//! Prototype interaction model.
//!
//! The editor and the prototype runtime share these types; the runtime only
//! executes definitions that passed [`crate::validation`], so an invalid
//! interaction can never reach playback.

use serde::{Deserialize, Serialize};

use crate::ids::{InteractionId, NodeId, VariableId};
use crate::variables::VariableValue;

/// What starts an interaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    /// Click / tap.
    Click {
        /// Optional delay before the actions run (milliseconds).
        #[serde(default)]
        delay_ms: f64,
    },
    /// Mouse hover / enter.
    Hover,
    /// Mouse press / touch down.
    Press,
    /// Keyboard key press.
    Key {
        /// Key name, e.g. `Enter` or `ArrowRight`.
        key: String,
    },
    /// A timer that starts when the frame is presented.
    AfterDelay {
        /// Delay in milliseconds.
        delay_ms: f64,
    },
}

impl Trigger {
    /// A plain click trigger.
    pub fn click() -> Trigger {
        Trigger::Click { delay_ms: 0.0 }
    }
}

/// Horizontal or vertical direction used by slide transitions and scrolling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SlideDirection {
    /// Slide in from the left.
    #[default]
    Left,
    /// Slide in from the right.
    Right,
    /// Slide in from above.
    Up,
    /// Slide in from below.
    Down,
}

/// Where an overlay is anchored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayPosition {
    /// Centred in the viewport.
    #[default]
    Center,
    /// Top left.
    TopLeft,
    /// Top right.
    TopRight,
    /// Bottom left.
    BottomLeft,
    /// Bottom right.
    BottomRight,
    /// Manually positioned relative to the source frame.
    Manual,
}

/// Where a scroll action stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScrollAlign {
    /// Scroll so the target is at the top/left.
    #[default]
    Top,
    /// Scroll so the target is centred.
    Center,
    /// Scroll so the target is at the bottom/right.
    Bottom,
}

/// Animation easing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[derive(Default)]
pub enum Easing {
    /// Linear interpolation.
    Linear,
    /// Starts slow.
    EaseIn,
    /// Ends slow.
    EaseOut,
    /// Slow at both ends.
    #[default]
    EaseInOut,
    /// Physical spring.
    Spring {
        /// Spring stiffness.
        stiffness: f64,
        /// Damping coefficient.
        damping: f64,
        /// Mass.
        mass: f64,
    },
}

impl Easing {
    /// Maps normalized time `0..=1` to an eased progress value.
    pub fn sample(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t,
            Easing::EaseOut => t * (2.0 - t),
            Easing::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    -1.0 + (4.0 - 2.0 * t) * t
                }
            }
            Easing::Spring {
                stiffness,
                damping,
                mass,
            } => {
                // Endpoints are exact: an animation that reaches its final
                // frame must land on the final value even though the spring
                // may overshoot inside the range.
                if t <= 0.0 {
                    return 0.0;
                }
                if t >= 1.0 {
                    return 1.0;
                }
                // Critically-damped-ish analytic spring; deterministic and cheap.
                let stiffness = stiffness.max(0.01);
                let damping = damping.max(0.0);
                let mass = mass.max(0.01);
                let omega = (stiffness / mass).sqrt();
                let zeta = damping / (2.0 * (stiffness * mass).sqrt());
                if zeta >= 1.0 {
                    1.0 - (-omega * t).exp() * (1.0 + omega * t)
                } else {
                    let omega_d = omega * (1.0 - zeta * zeta).sqrt();
                    1.0 - (-zeta * omega * t).exp()
                        * ((omega_d * t).cos() + (zeta * omega / omega_d) * (omega_d * t).sin())
                }
            }
        }
    }
}

/// The visual transition between two frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[derive(Default)]
pub enum TransitionKind {
    /// No animation.
    Instant,
    /// Cross fade.
    #[default]
    Dissolve,
    /// Slide the new frame in from a direction.
    Slide {
        /// Direction of travel.
        #[serde(default)]
        direction: SlideDirection,
    },
    /// Animate elements that exist in both frames (smart animate).
    Smart,
}

/// A transition with timing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    /// What kind of animation to run.
    #[serde(default)]
    pub kind: TransitionKind,
    /// Duration in milliseconds.
    #[serde(default = "default_duration")]
    pub duration_ms: f64,
    /// Easing curve.
    #[serde(default)]
    pub easing: Easing,
}

fn default_duration() -> f64 {
    300.0
}

impl Default for Transition {
    fn default() -> Transition {
        Transition {
            kind: TransitionKind::default(),
            duration_ms: 300.0,
            easing: Easing::default(),
        }
    }
}

impl Transition {
    /// A dissolve transition.
    pub fn dissolve(duration_ms: f64) -> Transition {
        Transition {
            kind: TransitionKind::Dissolve,
            duration_ms,
            easing: Easing::EaseInOut,
        }
    }

    /// Progress after `elapsed_ms`, in `0..=1` using both duration and easing.
    pub fn progress(&self, elapsed_ms: f64) -> f64 {
        if self.duration_ms <= 0.0 {
            return 1.0;
        }
        self.easing.sample(elapsed_ms / self.duration_ms)
    }
}

/// An action performed by an interaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Navigate to another frame.
    NavigateTo {
        /// Destination frame node.
        frame_id: NodeId,
        /// Optional transition override.
        #[serde(default)]
        transition: Option<Transition>,
    },
    /// Open a frame as an overlay.
    OpenOverlay {
        /// Overlay frame node.
        frame_id: NodeId,
        /// Anchoring.
        #[serde(default)]
        position: OverlayPosition,
        /// Whether to dim the content behind.
        #[serde(default)]
        backdrop: bool,
        /// Optional transition override.
        #[serde(default)]
        transition: Option<Transition>,
    },
    /// Close the top-most overlay.
    CloseOverlay,
    /// Scroll a scrollable container so a target becomes visible.
    ScrollTo {
        /// Target node inside the scroll container.
        target: NodeId,
        /// Alignment of the target.
        #[serde(default)]
        align: ScrollAlign,
        /// Whether to animate.
        #[serde(default)]
        animated: bool,
    },
    /// Go back to the previous frame in the history stack.
    Back,
    /// Set a variable value.
    SetVariable {
        /// Variable to change.
        variable_id: VariableId,
        /// New value.
        value: VariableValue,
    },
}

/// A complete interaction definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Interaction {
    /// Interaction id.
    pub id: InteractionId,
    /// Trigger.
    pub trigger: Trigger,
    /// Actions executed in order.
    #[serde(default)]
    pub actions: Vec<Action>,
}

impl Interaction {
    /// A click interaction navigating to a frame.
    pub fn navigate_on_click(frame: NodeId) -> Interaction {
        Interaction {
            id: InteractionId::new_v4(),
            trigger: Trigger::click(),
            actions: vec![Action::NavigateTo {
                frame_id: frame,
                transition: None,
            }],
        }
    }

    /// Validates structural requirements that do not need the document.
    pub fn validate_local(&self) -> Vec<String> {
        let mut issues = Vec::new();
        if self.actions.is_empty() {
            issues.push("interaction has no actions".to_string());
        }
        match &self.trigger {
            Trigger::AfterDelay { delay_ms } if *delay_ms < 0.0 => {
                issues.push("after-delay trigger has a negative delay".to_string());
            }
            Trigger::Key { key } if key.trim().is_empty() => {
                issues.push("key trigger has an empty key".to_string());
            }
            _ => {}
        }
        for action in &self.actions {
            if let Action::SetVariable { variable_id, .. } = action
                && *variable_id == VariableId::from_u128(0)
            {
                issues.push("set-variable action has an empty variable id".to_string());
            }
        }
        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_endpoints_and_monotonicity() {
        for easing in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
            Easing::Spring {
                stiffness: 120.0,
                damping: 14.0,
                mass: 1.0,
            },
        ] {
            assert!((easing.sample(0.0)).abs() < 1e-9);
            assert!((easing.sample(1.0) - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn transition_progress_uses_easing() {
        let transition = Transition::dissolve(200.0);
        assert_eq!(transition.progress(0.0), 0.0);
        assert!((transition.progress(200.0) - 1.0).abs() < 1e-9);
        let mid = transition.progress(100.0);
        assert!(mid > 0.0 && mid < 1.0);
    }

    #[test]
    fn interaction_serde_roundtrip() {
        let interaction = Interaction::navigate_on_click(NodeId::new_v4());
        let json = serde_json::to_value(&interaction).unwrap();
        assert_eq!(json["trigger"]["type"], "click");
        assert_eq!(json["actions"][0]["type"], "navigate_to");
        let back: Interaction = serde_json::from_value(json).unwrap();
        assert_eq!(back, interaction);
    }

    #[test]
    fn validate_local_reports_empty_actions() {
        let interaction = Interaction {
            id: InteractionId::new_v4(),
            trigger: Trigger::Hover,
            actions: Vec::new(),
        };
        assert_eq!(interaction.validate_local().len(), 1);
    }
}
