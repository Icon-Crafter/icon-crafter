//! Deterministic prototype runtime.
//!
//! The runtime executes the interaction model stored in [`dp_model`] — it is
//! the single implementation shared by the editor preview and shared prototype
//! links. It never mutates the document; instead it maintains presentation
//! state (current frame, overlay stack, navigation history, variables) and
//! emits [`RuntimeEvent`]s that a UI layer turns into pixels and animations.

use std::collections::HashMap;

use dp_model::{
    Action, Color, DevicePreset, Document, FlowId, Interaction, Node, NodeId, NodeKind,
    OverlayPosition, PrototypeFlow, ScrollAlign, Transition, TransitionKind, Trigger, VariableId,
    VariableValue,
};
use thiserror::Error;

use crate::expr::{Expr, ExprError, Value};

/// Ramp used when an action does not specify a transition.
pub const DEFAULT_TRANSITION_MS: f64 = 300.0;

/// Maximum number of nested overlays kept on the stack.
pub const MAX_OVERLAYS: usize = 16;

/// Maximum number of navigation history entries.
pub const MAX_HISTORY: usize = 128;

/// Errors produced by the prototype runtime.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum RuntimeError {
    /// The document has no frame to start from.
    #[error("document has no prototype start frame")]
    NoStartFrame,
    /// A referenced node does not exist.
    #[error("node {0} does not exist")]
    MissingNode(NodeId),
    /// A referenced node is not a frame.
    #[error("node {0} is not a frame")]
    NotAFrame(NodeId),
    /// A referenced variable does not exist.
    #[error("variable {0} does not exist")]
    MissingVariable(VariableId),
    /// A named flow does not exist.
    #[error("prototype flow {0} does not exist")]
    MissingFlow(FlowId),
    /// The overlay stack is full.
    #[error("too many nested overlays (limit {MAX_OVERLAYS})")]
    TooManyOverlays,
    /// An expression failed.
    #[error("expression error: {0}")]
    Expression(#[from] ExprError),
}

/// An open overlay frame.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayInstance {
    /// Runtime-local overlay id.
    pub id: usize,
    /// Frame rendered by the overlay.
    pub frame_id: NodeId,
    /// Where the overlay is anchored.
    pub position: OverlayPosition,
    /// Whether the content behind is dimmed.
    pub backdrop: bool,
    /// Runtime time (ms) when the overlay opened.
    pub opened_ms: f64,
    /// Optional open transition.
    pub transition: Option<Transition>,
}

impl OverlayInstance {
    /// Open animation progress in `0..=1` at runtime time `now_ms`.
    pub fn progress(&self, now_ms: f64) -> f64 {
        match &self.transition {
            Some(transition) => transition.progress(now_ms - self.opened_ms),
            None => 1.0,
        }
    }
}

/// A transition currently animating between two frames.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveTransition {
    /// Frame being left.
    pub from: NodeId,
    /// Frame being entered.
    pub to: NodeId,
    /// Transition parameters.
    pub transition: Transition,
    /// Elapsed time in milliseconds.
    pub elapsed_ms: f64,
}

impl ActiveTransition {
    /// Eased progress in `0..=1`.
    pub fn progress(&self) -> f64 {
        self.transition.progress(self.elapsed_ms)
    }

    /// Whether the animation has reached its end.
    pub fn is_finished(&self) -> bool {
        self.progress() >= 1.0
    }
}

/// Actions scheduled for a future runtime time.
#[derive(Debug, Clone, PartialEq)]
struct PendingActions {
    due_ms: f64,
    node_id: NodeId,
    actions: Vec<Action>,
}

/// Something that happened in the runtime and that the UI must present.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeEvent {
    /// A frame became the presented frame.
    Presented {
        /// Frame node.
        frame_id: NodeId,
    },
    /// Navigation to another frame.
    Navigated {
        /// Frame that was left.
        from: NodeId,
        /// Frame that is now presented.
        to: NodeId,
    },
    /// An overlay opened.
    OverlayOpened {
        /// Overlay id.
        overlay_id: usize,
        /// Overlay frame.
        frame_id: NodeId,
    },
    /// An overlay closed.
    OverlayClosed {
        /// Overlay id.
        overlay_id: usize,
        /// Overlay frame.
        frame_id: NodeId,
    },
    /// A scroll action targets a node.
    Scrolled {
        /// Node to scroll into view.
        target: NodeId,
        /// Alignment.
        align: ScrollAlign,
    },
    /// The runtime went back in history.
    WentBack {
        /// Frame now presented.
        to: NodeId,
    },
    /// A variable changed.
    VariableSet {
        /// Variable id.
        variable_id: VariableId,
    },
    /// A transition started.
    TransitionStarted {
        /// Frame being left.
        from: NodeId,
        /// Frame being entered.
        to: NodeId,
    },
    /// The active transition finished.
    TransitionFinished {
        /// Frame now presented.
        to: NodeId,
    },
    /// Actions were scheduled (trigger delay or timer).
    ActionsScheduled {
        /// Node owning the interaction.
        node_id: NodeId,
        /// Runtime time in milliseconds.
        due_ms: f64,
    },
}

/// The prototype runtime.
pub struct PrototypeRuntime<'a> {
    document: &'a Document,
    current: NodeId,
    history: Vec<NodeId>,
    overlays: Vec<OverlayInstance>,
    variables: HashMap<VariableId, VariableValue>,
    variable_names: HashMap<String, VariableId>,
    transition: Option<ActiveTransition>,
    pending: Vec<PendingActions>,
    events: Vec<RuntimeEvent>,
    now_ms: f64,
    next_overlay_id: usize,
    device: Option<DevicePreset>,
    background: Color,
}

impl<'a> PrototypeRuntime<'a> {
    /// Starts a runtime at `start_frame` (or the document's first start frame).
    pub fn start(
        document: &'a Document,
        start_frame: Option<NodeId>,
    ) -> Result<PrototypeRuntime<'a>, RuntimeError> {
        let current = resolve_start_frame(document, start_frame)?;
        let variables = initial_variables(document);
        let variable_names = document
            .variables
            .variables
            .iter()
            .map(|(id, variable)| (variable.name.clone(), *id))
            .collect();
        let mut runtime = PrototypeRuntime {
            document,
            current,
            history: Vec::new(),
            overlays: Vec::new(),
            variables,
            variable_names,
            transition: None,
            pending: Vec::new(),
            events: Vec::new(),
            now_ms: 0.0,
            next_overlay_id: 1,
            device: document.prototype.device.clone(),
            background: document.prototype.background,
        };
        runtime.events.push(RuntimeEvent::Presented {
            frame_id: runtime.current,
        });
        runtime.schedule_after_delay_triggers(runtime.current);
        Ok(runtime)
    }

    /// Starts a runtime for a named flow.
    pub fn start_flow(
        document: &'a Document,
        flow: FlowId,
    ) -> Result<PrototypeRuntime<'a>, RuntimeError> {
        let start = document
            .prototype
            .flows
            .iter()
            .find(|candidate| candidate.id == flow)
            .map(|flow| flow.start_frame_id)
            .ok_or(RuntimeError::MissingFlow(flow))?;
        PrototypeRuntime::start(document, Some(start))
    }

    /// The document being presented.
    pub fn document(&self) -> &'a Document {
        self.document
    }

    /// The presented frame.
    pub fn current_frame(&self) -> NodeId {
        self.current
    }

    /// The presented frame node.
    pub fn current_frame_node(&self) -> &'a Node {
        self.document
            .node(self.current)
            .expect("current frame is validated when it is set")
    }

    /// Device preset chosen by the document.
    pub fn device(&self) -> Option<&DevicePreset> {
        self.device.as_ref()
    }

    /// Presentation background.
    pub fn background(&self) -> Color {
        self.background
    }

    /// Navigation history (oldest first).
    pub fn history(&self) -> &[NodeId] {
        &self.history
    }

    /// Open overlays, bottom first.
    pub fn overlays(&self) -> &[OverlayInstance] {
        &self.overlays
    }

    /// The active transition, if any.
    pub fn active_transition(&self) -> Option<&ActiveTransition> {
        self.transition.as_ref()
    }

    /// Runtime clock in milliseconds.
    pub fn now_ms(&self) -> f64 {
        self.now_ms
    }

    /// Drains pending runtime events.
    pub fn drain_events(&mut self) -> Vec<RuntimeEvent> {
        std::mem::take(&mut self.events)
    }

    /// Peeks at pending runtime events.
    pub fn events(&self) -> &[RuntimeEvent] {
        &self.events
    }

    /// Interactions attached to a node.
    pub fn interactions_on(&self, node_id: NodeId) -> Vec<&'a Interaction> {
        self.document
            .node(node_id)
            .and_then(|node| node.prototype.as_ref())
            .map(|prototype| prototype.interactions.iter().collect())
            .unwrap_or_default()
    }

    /// Fires matching interactions on `node_id`.
    pub fn trigger(
        &mut self,
        node_id: NodeId,
        trigger: &Trigger,
    ) -> Result<Vec<RuntimeEvent>, RuntimeError> {
        let interactions: Vec<Interaction> = self
            .interactions_on(node_id)
            .into_iter()
            .filter(|interaction| triggers_match(&interaction.trigger, trigger))
            .cloned()
            .collect();
        let mut fired = Vec::new();
        for interaction in interactions {
            match interaction.trigger {
                Trigger::Click { delay_ms } if delay_ms > 0.0 => {
                    self.schedule(node_id, &interaction.actions, delay_ms);
                    fired.push(RuntimeEvent::ActionsScheduled {
                        node_id,
                        due_ms: self.now_ms + delay_ms,
                    });
                }
                _ => {
                    let mut events = Vec::new();
                    self.run_actions(&interaction.actions, &mut events)?;
                    fired.extend(events);
                }
            }
        }
        self.events.extend(fired.iter().cloned());
        Ok(fired)
    }

    /// Convenience: fires the click trigger.
    pub fn click(&mut self, node_id: NodeId) -> Result<Vec<RuntimeEvent>, RuntimeError> {
        self.trigger(node_id, &Trigger::click())
    }

    /// Convenience: fires a key trigger.
    pub fn press_key(
        &mut self,
        node_id: NodeId,
        key: &str,
    ) -> Result<Vec<RuntimeEvent>, RuntimeError> {
        self.trigger(
            node_id,
            &Trigger::Key {
                key: key.to_owned(),
            },
        )
    }

    /// Advances runtime time, finishing transitions and running timers.
    pub fn tick(&mut self, delta_ms: f64) -> Vec<RuntimeEvent> {
        self.now_ms += delta_ms.max(0.0);
        let mut events = Vec::new();

        let finished = self
            .transition
            .as_ref()
            .map(|transition| transition.is_finished())
            .unwrap_or(false);
        if finished {
            if let Some(transition) = self.transition.take() {
                self.history.push(transition.from);
                trim_history(&mut self.history);
                self.current = transition.to;
                events.push(RuntimeEvent::TransitionFinished { to: transition.to });
                self.schedule_after_delay_triggers(transition.to);
            }
        } else if let Some(transition) = self.transition.as_mut() {
            transition.elapsed_ms += delta_ms.max(0.0);
            if transition.is_finished() {
                let transition = self.transition.take().expect("just observed");
                self.history.push(transition.from);
                trim_history(&mut self.history);
                self.current = transition.to;
                events.push(RuntimeEvent::TransitionFinished { to: transition.to });
                self.schedule_after_delay_triggers(transition.to);
            }
        }

        let mut due: Vec<PendingActions> = {
            let mut due = Vec::new();
            let mut index = 0;
            while index < self.pending.len() {
                if self.pending[index].due_ms <= self.now_ms {
                    due.push(self.pending.remove(index));
                } else {
                    index += 1;
                }
            }
            due
        };
        due.sort_by(|a, b| a.due_ms.total_cmp(&b.due_ms));
        for pending in due {
            let mut action_events = Vec::new();
            if self
                .run_actions(&pending.actions, &mut action_events)
                .is_ok()
            {
                events.extend(action_events);
            }
        }

        self.events.extend(events.iter().cloned());
        events
    }

    /// Goes back to the previous frame.
    pub fn back(&mut self) -> Vec<RuntimeEvent> {
        if !self.overlays.is_empty() {
            return self.close_overlay().into_iter().collect();
        }
        let Some(previous) = self.history.pop() else {
            return Vec::new();
        };
        if !self.is_frame(previous) {
            return Vec::new();
        }
        let from = self.current;
        self.current = previous;
        self.transition = None;
        let events = vec![RuntimeEvent::WentBack { to: previous }];
        tracing::debug!(from = %from, to = %previous, "prototype back navigation");
        self.schedule_after_delay_triggers(previous);
        self.events.extend(events.iter().cloned());
        events
    }

    /// Closes the top-most overlay.
    pub fn close_overlay(&mut self) -> Option<RuntimeEvent> {
        let overlay = self.overlays.pop()?;
        let event = RuntimeEvent::OverlayClosed {
            overlay_id: overlay.id,
            frame_id: overlay.frame_id,
        };
        self.events.push(event.clone());
        Some(event)
    }

    /// Reads a runtime variable value.
    pub fn variable_value(&self, id: VariableId) -> Option<&VariableValue> {
        self.variables.get(&id)
    }

    /// Reads a variable by name.
    pub fn variable_value_by_name(&self, name: &str) -> Option<&VariableValue> {
        self.variable_names
            .get(name)
            .and_then(|id| self.variables.get(id))
    }

    /// Numeric view of a variable (floats and booleans only).
    pub fn numeric_variable(&self, name: &str) -> Option<f64> {
        match self.variable_value_by_name(name)? {
            VariableValue::Float { value } => Some(*value),
            VariableValue::Boolean { value } => Some(if *value { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    /// Sets a variable as if a `SetVariable` action ran.
    pub fn set_variable(&mut self, id: VariableId, value: VariableValue) -> bool {
        if !self.document.variables.variables.contains_key(&id) {
            return false;
        }
        self.variables.insert(id, value);
        self.events
            .push(RuntimeEvent::VariableSet { variable_id: id });
        true
    }

    /// Evaluates an expression using prototype variables by name.
    pub fn evaluate(&self, expression: &Expr) -> Result<Value, RuntimeError> {
        let value = expression.eval(&|name| self.variable_value_by_name(name).map(value_of))?;
        Ok(value)
    }

    /// Evaluates an expression to a number.
    pub fn evaluate_number(&self, expression: &Expr) -> Result<f64, RuntimeError> {
        let value = self.evaluate(expression)?;
        value
            .as_number()
            .ok_or_else(|| RuntimeError::Expression(ExprError::ExpectedNumber(value.display())))
    }

    /// Evaluates an expression to a boolean.
    pub fn evaluate_bool(&self, expression: &Expr) -> Result<bool, RuntimeError> {
        Ok(self.evaluate(expression)?.as_bool())
    }

    /// Validates that every action in a flow points at existing frames.
    pub fn validate_flow(document: &Document, flow: &PrototypeFlow) -> Vec<String> {
        let mut issues = Vec::new();
        if !is_frame(document, flow.start_frame_id) {
            issues.push(format!(
                "flow '{}' starts at node {} which is not a frame",
                flow.name, flow.start_frame_id
            ));
        }
        for (id, node) in &document.active_page().nodes {
            let Some(prototype) = &node.prototype else {
                continue;
            };
            for interaction in &prototype.interactions {
                for issue in interaction.validate_local() {
                    issues.push(format!("node {id}: {issue}"));
                }
                for action in &interaction.actions {
                    match action {
                        Action::NavigateTo { frame_id, .. } => {
                            if !is_frame(document, *frame_id) {
                                issues.push(format!(
                                    "node {id}: navigate target {frame_id} is not a frame"
                                ));
                            }
                        }
                        Action::OpenOverlay { frame_id, .. } => {
                            if !is_frame(document, *frame_id) {
                                issues.push(format!(
                                    "node {id}: overlay target {frame_id} is not a frame"
                                ));
                            }
                        }
                        Action::ScrollTo { target, .. } => {
                            if document.node(*target).is_none() {
                                issues.push(format!(
                                    "node {id}: scroll target {target} does not exist"
                                ));
                            }
                        }
                        Action::SetVariable { variable_id, .. } => {
                            if !document.variables.variables.contains_key(variable_id) {
                                issues.push(format!(
                                    "node {id}: set-variable target {variable_id} does not exist"
                                ));
                            }
                        }
                        Action::CloseOverlay | Action::Back => {}
                    }
                }
            }
        }
        issues
    }

    // ---------------------------------------------------------------------------------
    // Internal helpers.
    // ---------------------------------------------------------------------------------

    fn is_frame(&self, id: NodeId) -> bool {
        is_frame(self.document, id)
    }

    fn run_actions(
        &mut self,
        actions: &[Action],
        events: &mut Vec<RuntimeEvent>,
    ) -> Result<(), RuntimeError> {
        for action in actions {
            match action {
                Action::NavigateTo {
                    frame_id,
                    transition,
                } => {
                    let frame = self.frame_node(*frame_id)?;
                    let _ = frame;
                    if *frame_id == self.current && self.transition.is_none() {
                        continue;
                    }
                    let transition = transition
                        .clone()
                        .unwrap_or_else(|| Transition::dissolve(DEFAULT_TRANSITION_MS));
                    self.begin_navigation(*frame_id, transition, events);
                }
                Action::OpenOverlay {
                    frame_id,
                    position,
                    backdrop,
                    transition,
                } => {
                    self.frame_node(*frame_id)?;
                    if self.overlays.len() >= MAX_OVERLAYS {
                        return Err(RuntimeError::TooManyOverlays);
                    }
                    let overlay = OverlayInstance {
                        id: self.next_overlay_id,
                        frame_id: *frame_id,
                        position: *position,
                        backdrop: *backdrop,
                        opened_ms: self.now_ms,
                        transition: transition.clone(),
                    };
                    self.next_overlay_id += 1;
                    events.push(RuntimeEvent::OverlayOpened {
                        overlay_id: overlay.id,
                        frame_id: overlay.frame_id,
                    });
                    self.overlays.push(overlay);
                }
                Action::CloseOverlay => {
                    if let Some(overlay) = self.overlays.pop() {
                        events.push(RuntimeEvent::OverlayClosed {
                            overlay_id: overlay.id,
                            frame_id: overlay.frame_id,
                        });
                    }
                }
                Action::ScrollTo { target, align, .. } => {
                    if self.document.node(*target).is_none() {
                        return Err(RuntimeError::MissingNode(*target));
                    }
                    events.push(RuntimeEvent::Scrolled {
                        target: *target,
                        align: *align,
                    });
                }
                Action::Back => {
                    if let Some(overlay) = self.overlays.pop() {
                        events.push(RuntimeEvent::OverlayClosed {
                            overlay_id: overlay.id,
                            frame_id: overlay.frame_id,
                        });
                    } else if let Some(previous) = self.history.pop()
                        && self.is_frame(previous)
                    {
                        self.current = previous;
                        self.transition = None;
                        events.push(RuntimeEvent::WentBack { to: previous });
                        self.schedule_after_delay_triggers(previous);
                    }
                }
                Action::SetVariable { variable_id, value } => {
                    let value = self.resolve_assignment(*variable_id, value.clone())?;
                    self.variables.insert(*variable_id, value);
                    events.push(RuntimeEvent::VariableSet {
                        variable_id: *variable_id,
                    });
                }
            }
        }
        Ok(())
    }

    fn begin_navigation(
        &mut self,
        target: NodeId,
        transition: Transition,
        events: &mut Vec<RuntimeEvent>,
    ) {
        // Finish any in-flight transition first so the history stays linear.
        if let Some(active) = self.transition.take() {
            self.history.push(active.from);
            trim_history(&mut self.history);
        }
        if matches!(transition.kind, TransitionKind::Instant) || transition.duration_ms <= 0.0 {
            let from = self.current;
            self.history.push(from);
            trim_history(&mut self.history);
            self.current = target;
            self.transition = None;
            events.push(RuntimeEvent::Navigated { from, to: target });
            self.schedule_after_delay_triggers(target);
            return;
        }
        let from = self.current;
        self.transition = Some(ActiveTransition {
            from,
            to: target,
            transition,
            elapsed_ms: 0.0,
        });
        events.push(RuntimeEvent::TransitionStarted { from, to: target });
    }

    fn schedule(&mut self, node_id: NodeId, actions: &[Action], delay_ms: f64) {
        if actions.is_empty() {
            return;
        }
        self.pending.push(PendingActions {
            due_ms: self.now_ms + delay_ms.max(0.0),
            node_id,
            actions: actions.to_vec(),
        });
    }

    fn schedule_after_delay_triggers(&mut self, frame: NodeId) {
        let mut scheduled = Vec::new();
        collect_after_delay(self.document, frame, &mut scheduled);
        for (node_id, delay_ms, actions) in scheduled {
            self.schedule(node_id, &actions, delay_ms);
        }
    }

    fn resolve_assignment(
        &self,
        variable_id: VariableId,
        value: VariableValue,
    ) -> Result<VariableValue, RuntimeError> {
        let variable = self
            .document
            .variables
            .variables
            .get(&variable_id)
            .ok_or(RuntimeError::MissingVariable(variable_id))?;
        match value {
            VariableValue::Alias {
                variable_id: target,
            } => {
                let collection = self
                    .document
                    .variables
                    .collection(variable.collection_id)
                    .ok_or(RuntimeError::MissingVariable(target))?;
                self.document
                    .variables
                    .resolve(target, collection.default_mode)
                    .ok_or(RuntimeError::MissingVariable(target))
            }
            concrete => Ok(concrete),
        }
    }

    fn frame_node(&self, id: NodeId) -> Result<&'a Node, RuntimeError> {
        let node = self
            .document
            .node(id)
            .ok_or(RuntimeError::MissingNode(id))?;
        if node.kind != NodeKind::Frame {
            return Err(RuntimeError::NotAFrame(id));
        }
        Ok(node)
    }
}

fn value_of(value: &VariableValue) -> Value {
    match value {
        VariableValue::Color { color } => Value::Text(color.to_hex()),
        VariableValue::Float { value } => Value::Number(*value),
        VariableValue::String { value } => Value::Text(value.clone()),
        VariableValue::Boolean { value } => Value::Bool(*value),
        VariableValue::Alias { variable_id } => Value::Text(format!("{variable_id}")),
    }
}

fn is_frame(document: &Document, id: NodeId) -> bool {
    document
        .node(id)
        .map(|node| node.kind == NodeKind::Frame)
        .unwrap_or(false)
}

fn resolve_start_frame(
    document: &Document,
    explicit: Option<NodeId>,
) -> Result<NodeId, RuntimeError> {
    let candidate = explicit
        .or_else(|| document.prototype.start_frames.first().copied())
        .or_else(|| {
            document
                .prototype
                .flows
                .first()
                .map(|flow| flow.start_frame_id)
        })
        .or_else(|| first_frame(document));
    let candidate = candidate.ok_or(RuntimeError::NoStartFrame)?;
    if document.node(candidate).is_none() {
        return Err(RuntimeError::MissingNode(candidate));
    }
    if !is_frame(document, candidate) {
        return Err(RuntimeError::NotAFrame(candidate));
    }
    Ok(candidate)
}

fn first_frame(document: &Document) -> Option<NodeId> {
    let page = document.active_page();
    page.nodes
        .iter()
        .find(|(_, node)| node.kind == NodeKind::Frame)
        .map(|(id, _)| *id)
}

fn collect_after_delay(
    document: &Document,
    node_id: NodeId,
    out: &mut Vec<(NodeId, f64, Vec<Action>)>,
) {
    let Some(node) = document.node(node_id) else {
        return;
    };
    if let Some(prototype) = &node.prototype {
        for interaction in &prototype.interactions {
            if let Trigger::AfterDelay { delay_ms } = interaction.trigger {
                out.push((node_id, delay_ms, interaction.actions.clone()));
            }
        }
    }
    for child in &node.children {
        collect_after_delay(document, *child, out);
    }
}

fn triggers_match(defined: &Trigger, fired: &Trigger) -> bool {
    match (defined, fired) {
        (Trigger::Click { .. }, Trigger::Click { .. }) => true,
        (Trigger::Hover, Trigger::Hover) => true,
        (Trigger::Press, Trigger::Press) => true,
        (Trigger::Key { key: a }, Trigger::Key { key: b }) => a.eq_ignore_ascii_case(b),
        (Trigger::AfterDelay { .. }, Trigger::AfterDelay { .. }) => false,
        _ => false,
    }
}

fn trim_history(history: &mut Vec<NodeId>) {
    if history.len() > MAX_HISTORY {
        let excess = history.len() - MAX_HISTORY;
        history.drain(0..excess);
    }
}

fn initial_variables(document: &Document) -> HashMap<VariableId, VariableValue> {
    let mut variables = HashMap::new();
    for (id, variable) in &document.variables.variables {
        let mode = document
            .variables
            .collection(variable.collection_id)
            .map(|collection| collection.default_mode);
        let Some(mode) = mode else {
            continue;
        };
        if let Some(value) = document.variables.resolve(*id, mode)
            && !matches!(value, VariableValue::Alias { .. })
        {
            variables.insert(*id, value);
        }
    }
    variables
}

#[cfg(test)]
mod tests {
    use dp_model::{
        Action, Document, Easing, Interaction, NodeId, NodeKind, OverlayPosition, Transition,
        TransitionKind, Trigger, VariableValue,
    };

    use super::*;
    use crate::expr::Expr;

    fn document_with_frames() -> (Document, NodeId, NodeId, NodeId) {
        let mut document = Document::new("Prototype");
        let page = document.active_page_mut();
        let first = dp_model::Node::frame("First", 0.0, 0.0, 390.0, 844.0);
        let first_id = first.id;
        page.insert_node(first, None, None).unwrap();
        let second = dp_model::Node::frame("Second", 500.0, 0.0, 390.0, 844.0);
        let second_id = second.id;
        page.insert_node(second, None, None).unwrap();
        let button = dp_model::Node::rectangle("Button", 20.0, 20.0, 120.0, 40.0);
        let button_id = button.id;
        page.insert_node(button, Some(first_id), None).unwrap();
        document.prototype.start_frames = vec![first_id];
        (document, first_id, second_id, button_id)
    }

    fn add_interaction(document: &mut Document, node: NodeId, interaction: Interaction) {
        let page = document.active_page_mut();
        let target = page.nodes.get_mut(&node).unwrap();
        let prototype = target.prototype.get_or_insert_with(Default::default);
        prototype.interactions.push(interaction);
    }

    #[test]
    fn starts_at_configured_frame() {
        let (document, first, _, _) = document_with_frames();
        let runtime = PrototypeRuntime::start(&document, None).unwrap();
        assert_eq!(runtime.current_frame(), first);
        assert_eq!(runtime.current_frame_node().kind, NodeKind::Frame);
    }

    #[test]
    fn click_navigates_and_back_returns() {
        let (mut document, first, second, button) = document_with_frames();
        add_interaction(
            &mut document,
            button,
            Interaction {
                id: dp_model::InteractionId::new_v4(),
                trigger: Trigger::click(),
                actions: vec![Action::NavigateTo {
                    frame_id: second,
                    transition: Some(Transition {
                        kind: TransitionKind::Instant,
                        duration_ms: 0.0,
                        easing: Easing::Linear,
                    }),
                }],
            },
        );
        let mut runtime = PrototypeRuntime::start(&document, Some(first)).unwrap();
        runtime.drain_events();
        let events = runtime.click(button).unwrap();
        assert!(matches!(
            events.as_slice(),
            [RuntimeEvent::Navigated { from, to }] if *from == first && *to == second
        ));
        assert_eq!(runtime.current_frame(), second);
        let events = runtime.back();
        assert!(matches!(events.as_slice(), [RuntimeEvent::WentBack { to }] if *to == first));
        assert_eq!(runtime.current_frame(), first);
    }

    #[test]
    fn animated_transition_advances_with_ticks() {
        let (mut document, first, second, button) = document_with_frames();
        add_interaction(
            &mut document,
            button,
            Interaction::navigate_on_click(second),
        );
        let mut runtime = PrototypeRuntime::start(&document, Some(first)).unwrap();
        let events = runtime.click(button).unwrap();
        assert!(matches!(
            events.as_slice(),
            [RuntimeEvent::TransitionStarted { .. }]
        ));
        let mid = runtime.tick(150.0);
        assert!(mid.is_empty());
        assert!(
            runtime
                .active_transition()
                .map(|transition| transition.progress() > 0.0 && transition.progress() < 1.0)
                .unwrap()
        );
        let done = runtime.tick(200.0);
        assert!(matches!(
            done.as_slice(),
            [RuntimeEvent::TransitionFinished { to }] if *to == second
        ));
        assert_eq!(runtime.current_frame(), second);
    }

    #[test]
    fn overlays_open_and_close() {
        let (mut document, first, second, button) = document_with_frames();
        add_interaction(
            &mut document,
            button,
            Interaction {
                id: dp_model::InteractionId::new_v4(),
                trigger: Trigger::click(),
                actions: vec![
                    Action::OpenOverlay {
                        frame_id: second,
                        position: OverlayPosition::Center,
                        backdrop: true,
                        transition: None,
                    },
                    Action::CloseOverlay,
                ],
            },
        );
        let mut runtime = PrototypeRuntime::start(&document, Some(first)).unwrap();
        let events = runtime.click(button).unwrap();
        assert!(matches!(
            events.as_slice(),
            [
                RuntimeEvent::OverlayOpened { .. },
                RuntimeEvent::OverlayClosed { .. }
            ]
        ));
        assert!(runtime.overlays().is_empty());
    }

    #[test]
    fn key_trigger_matches_case_insensitively() {
        let (mut document, _, second, button) = document_with_frames();
        add_interaction(
            &mut document,
            button,
            Interaction {
                id: dp_model::InteractionId::new_v4(),
                trigger: Trigger::Key {
                    key: "Enter".into(),
                },
                actions: vec![Action::NavigateTo {
                    frame_id: second,
                    transition: None,
                }],
            },
        );
        let mut runtime = PrototypeRuntime::start(&document, None).unwrap();
        let events = runtime.press_key(button, "enter").unwrap();
        assert!(matches!(
            events.as_slice(),
            [RuntimeEvent::TransitionStarted { .. }]
        ));
    }

    #[test]
    fn after_delay_trigger_fires_on_without_tick() {
        let (mut document, first, second, button) = document_with_frames();
        add_interaction(
            &mut document,
            button,
            Interaction {
                id: dp_model::InteractionId::new_v4(),
                trigger: Trigger::AfterDelay { delay_ms: 100.0 },
                actions: vec![Action::NavigateTo {
                    frame_id: second,
                    transition: Some(Transition {
                        kind: TransitionKind::Instant,
                        duration_ms: 0.0,
                        easing: Easing::Linear,
                    }),
                }],
            },
        );
        let mut runtime = PrototypeRuntime::start(&document, Some(first)).unwrap();
        runtime.drain_events();
        assert!(runtime.tick(50.0).is_empty());
        let events = runtime.tick(60.0);
        assert!(matches!(
            events.as_slice(),
            [RuntimeEvent::Navigated { to, .. }] if *to == second
        ));
    }

    #[test]
    fn set_variable_action_updates_value_and_expression_reads_it() {
        let (mut document, first, _, button) = document_with_frames();
        let collection = dp_model::variables::VariableCollection::new("Theme");
        let mode = collection.default_mode;
        let collection_id = collection.id;
        document.variables.collections.push(collection);
        let variable = dp_model::variables::Variable::color(
            collection_id,
            "tone",
            mode,
            dp_model::Color::from_hex("#101010").unwrap(),
        );
        let variable_id = variable.id;
        document.variables.variables.insert(variable_id, variable);
        add_interaction(
            &mut document,
            button,
            Interaction {
                id: dp_model::InteractionId::new_v4(),
                trigger: Trigger::click(),
                actions: vec![Action::SetVariable {
                    variable_id,
                    value: VariableValue::Float { value: 42.0 },
                }],
            },
        );
        let mut runtime = PrototypeRuntime::start(&document, Some(first)).unwrap();
        let events = runtime.click(button).unwrap();
        assert!(matches!(
            events.as_slice(),
            [RuntimeEvent::VariableSet { .. }]
        ));
        assert_eq!(runtime.numeric_variable("tone"), Some(42.0));
        let expression = Expr::parse("tone / 2 + 1").unwrap();
        assert_eq!(runtime.evaluate_number(&expression).unwrap(), 22.0);
    }

    #[test]
    fn start_frame_for_flow() {
        let (mut document, _, second, _) = document_with_frames();
        let flow = PrototypeFlow::new("Alt", second);
        let flow_id = flow.id;
        document.prototype.flows.push(flow);
        let runtime = PrototypeRuntime::start_flow(&document, flow_id).unwrap();
        assert_eq!(runtime.current_frame(), second);
    }

    #[test]
    fn invalid_start_frame_is_rejected() {
        let (document, _, _, button) = document_with_frames();
        let error = PrototypeRuntime::start(&document, Some(button))
            .err()
            .expect("starting at a rectangle must fail");
        assert!(matches!(error, RuntimeError::NotAFrame(_)));
    }

    #[test]
    fn validate_flow_reports_broken_targets() {
        let (mut document, _, _, button) = document_with_frames();
        let mut flow = PrototypeFlow::new("Broken", button);
        add_interaction(
            &mut document,
            button,
            Interaction {
                id: dp_model::InteractionId::new_v4(),
                trigger: Trigger::click(),
                actions: vec![Action::NavigateTo {
                    frame_id: NodeId::new_v4(),
                    transition: None,
                }],
            },
        );
        let issues = PrototypeRuntime::validate_flow(&document, &flow);
        assert!(issues.iter().any(|issue| issue.contains("is not a frame")));
        flow.start_frame_id = button;
        let issues = PrototypeRuntime::validate_flow(&document, &flow);
        assert!(!issues.is_empty());
    }
}
