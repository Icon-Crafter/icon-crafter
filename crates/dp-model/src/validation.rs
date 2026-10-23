//! Document validation.
//!
//! Validation is split into hard errors (the document cannot be trusted) and
//! warnings (it can be opened, but some data is suspicious or dangling). The
//! editor surfaces both; importers and the collaboration server reject errors.

use serde::{Deserialize, Serialize};

use crate::document::{Document, SCHEMA_VERSION};
use crate::geometry::NodeGeometry;
use crate::node::{ComponentRole, Node, NodeKind};
use crate::paint::Effect;
use crate::prototype::Action;
use crate::variables::VariableValue;

/// Limits enforced by the document format (see `docs/DOCUMENT-FORMAT.md`).
pub const MAX_NODES: usize = 2_000_000;
/// Maximum anchors across all contours of a single path.
pub const MAX_PATH_ANCHORS: usize = 100_000;
/// Maximum number of contours in a single path.
pub const MAX_CONTOURS: usize = 10_000;
/// Maximum byte length of names and text content.
pub const MAX_STRING_BYTES: usize = 2 * 1024 * 1024;
/// Maximum nesting depth of the node tree.
pub const MAX_DEPTH: usize = 64;

/// Severity of a validation issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    /// The document is structurally invalid. Importers must reject it.
    Error,
    /// The document can be opened, but something is dangling or suspicious.
    Warning,
}

/// A single validation finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// How serious the finding is.
    pub severity: ValidationSeverity,
    /// Machine-stable identifier of the rule, e.g. `broken-style-reference`.
    pub code: &'static str,
    /// Human readable explanation.
    pub message: String,
    /// Path of the offending object, e.g. `pages[0]/node/<uuid>`.
    pub path: String,
}

impl ValidationIssue {
    /// Whether this issue is a hard error.
    pub fn is_error(&self) -> bool {
        self.severity == ValidationSeverity::Error
    }

    /// Whether this issue is a warning.
    pub fn is_warning(&self) -> bool {
        self.severity == ValidationSeverity::Warning
    }
}

fn error(issues: &mut Vec<ValidationIssue>, code: &'static str, path: String, message: String) {
    issues.push(ValidationIssue {
        severity: ValidationSeverity::Error,
        code,
        message,
        path,
    });
}

fn warning(issues: &mut Vec<ValidationIssue>, code: &'static str, path: String, message: String) {
    issues.push(ValidationIssue {
        severity: ValidationSeverity::Warning,
        code,
        message,
        path,
    });
}

/// Validates a whole document, returning every finding.
pub fn validate(document: &Document) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if document.schema != SCHEMA_VERSION {
        error(
            &mut issues,
            "schema-version",
            "schema".to_string(),
            format!(
                "document schema is {} but this build supports {}",
                document.schema, SCHEMA_VERSION
            ),
        );
    }
    if document.pages.is_empty() {
        error(
            &mut issues,
            "no-pages",
            "pages".to_string(),
            "a document must contain at least one page".to_string(),
        );
        return issues;
    }
    if document.pages.len() > 500 {
        warning(
            &mut issues,
            "many-pages",
            "pages".to_string(),
            format!("{} pages is unusually many", document.pages.len()),
        );
    }
    if !document
        .pages
        .iter()
        .any(|page| page.id == document.active_page)
    {
        error(
            &mut issues,
            "active-page-missing",
            "active_page".to_string(),
            format!("active page {} does not exist", document.active_page),
        );
    }
    if let Err(problem) = document.unique_node_ids() {
        error(
            &mut issues,
            "duplicate-node-id",
            "pages".to_string(),
            problem.to_string(),
        );
    }
    let mut total_nodes = 0usize;
    for (index, page) in document.pages.iter().enumerate() {
        let base = format!("pages[{index}]");
        for problem in page.check_integrity() {
            error(
                &mut issues,
                "page-integrity",
                format!("{base}/{}", page.id),
                problem,
            );
        }
        total_nodes = total_nodes.saturating_add(page.nodes.len());
        for node in page.nodes.values() {
            let path = format!("{base}/node/{}", node.id);
            if let Some(depth) = page.depth(node.id) {
                if depth > MAX_DEPTH {
                    warning(
                        &mut issues,
                        "deep-nesting",
                        path.clone(),
                        format!("node depth {depth} exceeds {MAX_DEPTH}"),
                    );
                }
            }
            validate_node(document, &path, node, &mut issues);
        }
    }
    if total_nodes > MAX_NODES {
        error(
            &mut issues,
            "too-many-nodes",
            "pages".to_string(),
            format!("{total_nodes} nodes exceeds the {MAX_NODES} limit"),
        );
    }
    validate_registries(document, &mut issues);
    validate_variables(document, &mut issues);
    issues
}

fn validate_node(
    document: &Document,
    path: &str,
    node: &Node,
    issues: &mut Vec<ValidationIssue>,
) {
    if node.name.trim().is_empty() {
        warning(
            issues,
            "empty-name",
            path.to_string(),
            "node has an empty name".to_string(),
        );
    }
    if node.name.len() > MAX_STRING_BYTES {
        error(
            issues,
            "name-too-long",
            path.to_string(),
            format!("name is {} bytes (max {MAX_STRING_BYTES})", node.name.len()),
        );
    }
    let transform = &node.transform;
    if !transform.is_finite() {
        error(
            issues,
            "non-finite-transform",
            path.to_string(),
            "transform contains NaN or infinity".to_string(),
        );
    } else if transform.w < 0.0 || transform.h < 0.0 {
        error(
            issues,
            "negative-size",
            path.to_string(),
            format!("size is {}x{}", transform.w, transform.h),
        );
    } else if transform.w == 0.0 || transform.h == 0.0 {
        warning(
            issues,
            "zero-size",
            path.to_string(),
            "node has a zero width or height".to_string(),
        );
    }
    if !node.opacity.is_finite() {
        error(
            issues,
            "non-finite-opacity",
            path.to_string(),
            "opacity is not a number".to_string(),
        );
    } else if !(0.0..=1.0).contains(&node.opacity) {
        warning(
            issues,
            "opacity-out-of-range",
            path.to_string(),
            format!("opacity {} is outside 0..1 and will be clamped", node.opacity),
        );
    }
    validate_style(document, path, node, issues);
    validate_geometry(path, node, issues);
    validate_text(document, path, node, issues);
    validate_layout(path, node, issues);
    validate_component_role(document, path, node, issues);
    validate_prototype(document, path, node, issues);
    match (&node.kind, &node.geometry) {
        (NodeKind::Vector, None) => warning(
            issues,
            "vector-without-path",
            path.to_string(),
            "vector node has no path geometry".to_string(),
        ),
        (NodeKind::Text, None) => warning(
            issues,
            "text-without-content",
            path.to_string(),
            "text node has no text content".to_string(),
        ),
        _ => {}
    }
}

fn validate_style(
    document: &Document,
    path: &str,
    node: &Node,
    issues: &mut Vec<ValidationIssue>,
) {
    for (index, fill) in node.style.fills.iter().enumerate() {
        let fill_path = format!("{path}/style/fills[{index}]");
        if let Some(style_id) = fill.style_id {
            if !document.styles.paints.contains_key(&style_id) {
                warning(
                    issues,
                    "broken-style-reference",
                    fill_path.clone(),
                    format!("fill references missing paint style {style_id}"),
                );
            }
        }
        if let Some(variable_id) = fill.variable_id {
            if document.variables.variable(variable_id).is_none() {
                warning(
                    issues,
                    "broken-variable-reference",
                    fill_path.clone(),
                    format!("fill references missing variable {variable_id}"),
                );
            }
        }
        for asset_id in fill.paint.asset_ids() {
            if !document.assets.images.contains_key(&asset_id) {
                warning(
                    issues,
                    "missing-image-asset",
                    fill_path.clone(),
                    format!("image fill references missing asset {asset_id}"),
                );
            }
        }
    }
    for (index, stroke) in node.style.strokes.iter().enumerate() {
        let stroke_path = format!("{path}/style/strokes[{index}]");
        if stroke.weight <= 0.0 || !stroke.weight.is_finite() {
            warning(
                issues,
                "invalid-stroke-weight",
                stroke_path.clone(),
                format!("stroke weight {} is not a positive number", stroke.weight),
            );
        }
        if let Some(style_id) = stroke.style_id {
            if !document.styles.paints.contains_key(&style_id) {
                warning(
                    issues,
                    "broken-style-reference",
                    stroke_path.clone(),
                    format!("stroke references missing paint style {style_id}"),
                );
            }
        }
        for asset_id in stroke.paint.asset_ids() {
            if !document.assets.images.contains_key(&asset_id) {
                warning(
                    issues,
                    "missing-image-asset",
                    stroke_path.clone(),
                    format!("stroke image references missing asset {asset_id}"),
                );
            }
        }
        if let Some(dash) = &stroke.dash {
            if dash.iter().any(|entry| *entry < 0.0 || !entry.is_finite()) {
                warning(
                    issues,
                    "invalid-dash",
                    stroke_path.clone(),
                    "dash pattern contains negative or non-finite entries".to_string(),
                );
            }
        }
    }
    for (index, effect) in node.style.effects.iter().enumerate() {
        let effect_path = format!("{path}/style/effects[{index}]");
        if let Some(radius) = effect.blur_radius() {
            if !radius.is_finite() {
                error(
                    issues,
                    "non-finite-effect",
                    effect_path.clone(),
                    "blur radius is not a number".to_string(),
                );
            }
        }
        match effect {
            Effect::DropShadow { blur, spread, offset, .. }
            | Effect::InnerShadow { blur, spread, offset, .. } => {
                if *blur < 0.0 || !blur.is_finite() {
                    warning(
                        issues,
                        "negative-shadow-blur",
                        effect_path.clone(),
                        format!("shadow blur {blur} is negative"),
                    );
                }
                if *spread < 0.0 {
                    warning(
                        issues,
                        "negative-shadow-spread",
                        effect_path.clone(),
                        format!("shadow spread {spread} is negative"),
                    );
                }
                if !offset.x.is_finite() || !offset.y.is_finite() {
                    error(
                        issues,
                        "non-finite-shadow-offset",
                        effect_path,
                        "shadow offset is not a number".to_string(),
                    );
                }
            }
            Effect::LayerBlur { .. } | Effect::BackgroundBlur { .. } => {}
        }
    }
    let radii = node.style.corner_radii;
    for (name, value) in [
        ("tl", radii.tl),
        ("tr", radii.tr),
        ("br", radii.br),
        ("bl", radii.bl),
    ] {
        if !value.is_finite() {
            error(
                issues,
                "non-finite-radius",
                format!("{path}/style/corner_radii"),
                format!("corner radius {name} is not a number"),
            );
        } else if value < 0.0 {
            warning(
                issues,
                "negative-radius",
                format!("{path}/style/corner_radii"),
                format!("corner radius {name} is negative"),
            );
        }
    }
}

fn validate_geometry(
    path: &str,
    node: &Node,
    issues: &mut Vec<ValidationIssue>,
) {
    let Some(geometry) = &node.geometry else {
        return;
    };
    match geometry {
        NodeGeometry::Vector { path: vector } => {
            if vector.contours.len() > MAX_CONTOURS {
                warning(
                    issues,
                    "too-many-contours",
                    path.to_string(),
                    format!(
                        "path has {} contours (max {MAX_CONTOURS})",
                        vector.contours.len()
                    ),
                );
            }
            let mut anchors = 0usize;
            for contour in &vector.contours {
                anchors = anchors.saturating_add(contour.anchors.len());
            }
            if anchors > MAX_PATH_ANCHORS {
                error(
                    issues,
                    "too-many-anchors",
                    path.to_string(),
                    format!("path has {anchors} anchors (max {MAX_PATH_ANCHORS})"),
                );
            }
            for (contour_index, contour) in vector.contours.iter().enumerate() {
                if contour.anchors.len() < 2 {
                    warning(
                        issues,
                        "short-contour",
                        format!("{path}/geometry/contours[{contour_index}]"),
                        "contour has fewer than two anchors".to_string(),
                    );
                }
                for (anchor_index, anchor) in contour.anchors.iter().enumerate() {
                    let finite = |point: dp_geom::Point| {
                        point.x.is_finite() && point.y.is_finite()
                    };
                    if !finite(anchor.point)
                        || !finite(anchor.in_handle)
                        || !finite(anchor.out_handle)
                    {
                        error(
                            issues,
                            "non-finite-anchor",
                            format!("{path}/geometry/contours[{contour_index}]/anchors[{anchor_index}]"),
                            "anchor or handle is not a number".to_string(),
                        );
                    }
                }
            }
        }
        NodeGeometry::Polygon { point_count } => {
            if !(3..=512).contains(point_count) {
                warning(
                    issues,
                    "invalid-polygon",
                    path.to_string(),
                    format!("polygon has {point_count} points; it will be clamped to 3..512"),
                );
            }
        }
        NodeGeometry::Star {
            point_count,
            inner_ratio,
        } => {
            if !(3..=512).contains(point_count) {
                warning(
                    issues,
                    "invalid-star",
                    path.to_string(),
                    format!("star has {point_count} points; it will be clamped to 3..512"),
                );
            }
            if !inner_ratio.is_finite() || !(0.05..=0.95).contains(inner_ratio) {
                warning(
                    issues,
                    "invalid-star-ratio",
                    path.to_string(),
                    format!("star inner ratio {inner_ratio} is outside 0.05..0.95"),
                );
            }
        }
        NodeGeometry::Boolean { .. } => {
            if node.children.len() < 2 {
                warning(
                    issues,
                    "boolean-too-few-operands",
                    path.to_string(),
                    "boolean node has fewer than two operands".to_string(),
                );
            }
        }
    }
}

fn validate_text(
    document: &Document,
    path: &str,
    node: &Node,
    issues: &mut Vec<ValidationIssue>,
) {
    let Some(text) = &node.text else {
        return;
    };
    if text.characters.len() > MAX_STRING_BYTES {
        error(
            issues,
            "text-too-long",
            format!("{path}/text"),
            format!(
                "text content is {} bytes (max {MAX_STRING_BYTES})",
                text.characters.len()
            ),
        );
    }
    let style = &text.style;
    if !style.font_size.is_finite() || style.font_size <= 0.0 {
        error(
            issues,
            "invalid-font-size",
            format!("{path}/text/style"),
            format!("font size {} must be positive", style.font_size),
        );
    }
    if !(100..=900).contains(&style.font_weight) {
        warning(
            issues,
            "invalid-font-weight",
            format!("{path}/text/style"),
            format!(
                "font weight {} is outside the usual 100..900 range",
                style.font_weight
            ),
        );
    }
    if !style.line_height.value.is_finite() || style.line_height.value < 0.0 {
        warning(
            issues,
            "invalid-line-height",
            format!("{path}/text/style"),
            "line height is negative or not a number".to_string(),
        );
    }
    if style.fills.is_empty() {
        warning(
            issues,
            "text-without-fill",
            format!("{path}/text/style"),
            "text has no fill and will be invisible".to_string(),
        );
    }
    for fill in &style.fills {
        for asset_id in fill.paint.asset_ids() {
            if !document.assets.images.contains_key(&asset_id) {
                warning(
                    issues,
                    "missing-image-asset",
                    format!("{path}/text/style"),
                    format!("text fill references missing asset {asset_id}"),
                );
            }
        }
    }
    if let Some(max_lines) = text.paragraph.max_lines {
        if max_lines == 0 {
            warning(
                issues,
                "zero-max-lines",
                format!("{path}/text/paragraph"),
                "max_lines is zero, so no text will be shown".to_string(),
            );
        }
    }
    if let Some(on_path) = &text.on_path {
        match document.node(on_path.path_node_id) {
            None => warning(
                issues,
                "text-on-path-missing",
                format!("{path}/text/on_path"),
                format!("text-path node {} does not exist", on_path.path_node_id),
            ),
            Some(target) if target.kind != NodeKind::Vector => warning(
                issues,
                "text-on-path-not-vector",
                format!("{path}/text/on_path"),
                format!("text-path node {} is not a vector path", on_path.path_node_id),
            ),
            Some(_) => {}
        }
        if on_path.path_node_id == node.id {
            error(
                issues,
                "text-on-own-path",
                format!("{path}/text/on_path"),
                "a text node cannot use its own geometry as text path".to_string(),
            );
        }
    }
}

fn validate_layout(path: &str, node: &Node, issues: &mut Vec<ValidationIssue>) {
    let Some(layout) = &node.layout else {
        return;
    };
    if !layout.gap.is_finite() || layout.gap < 0.0 {
        warning(
            issues,
            "invalid-gap",
            format!("{path}/layout"),
            format!("layout gap {} is negative", layout.gap),
        );
    }
    let padding = layout.padding;
    for (name, value) in [
        ("top", padding.top),
        ("right", padding.right),
        ("bottom", padding.bottom),
        ("left", padding.left),
    ] {
        if !value.is_finite() || value < 0.0 {
            warning(
                issues,
                "invalid-padding",
                format!("{path}/layout"),
                format!("layout padding {name} = {value} is invalid"),
            );
        }
    }
    if let (Some(min), Some(max)) = (layout.min_size, layout.max_size) {
        if min.w > max.w {
            error(
                issues,
                "min-greater-than-max",
                format!("{path}/layout"),
                format!("minimum width {} exceeds maximum width {}", min.w, max.w),
            );
        }
        if min.h > max.h {
            error(
                issues,
                "min-greater-than-max",
                format!("{path}/layout"),
                format!("minimum height {} exceeds maximum height {}", min.h, max.h),
            );
        }
    }
}

fn validate_component_role(
    document: &Document,
    path: &str,
    node: &Node,
    issues: &mut Vec<ValidationIssue>,
) {
    match &node.component {
        Some(ComponentRole::Instance {
            component_id,
            swapped_component_id,
            ..
        }) => {
            if !document.components.contains_component(*component_id) {
                warning(
                    issues,
                    "broken-instance",
                    path.to_string(),
                    format!("instance references missing component {component_id}"),
                );
            }
            if let Some(swapped) = swapped_component_id {
                if !document.components.contains_component(*swapped) {
                    warning(
                        issues,
                        "broken-swapped-instance",
                        path.to_string(),
                        format!("instance swaps to missing component {swapped}"),
                    );
                }
            }
        }
        Some(ComponentRole::Component { .. }) => {
            if node.kind != NodeKind::Component {
                warning(
                    issues,
                    "component-role-mismatch",
                    path.to_string(),
                    format!("component role on a {:?} node", node.kind),
                );
            }
            if document.components.component_for_node(node.id).is_none() {
                warning(
                    issues,
                    "unregistered-component",
                    path.to_string(),
                    "component node has no registry entry".to_string(),
                );
            }
        }
        Some(ComponentRole::ComponentSet { .. }) => {
            if node.kind != NodeKind::ComponentSet {
                warning(
                    issues,
                    "component-set-role-mismatch",
                    path.to_string(),
                    format!("component-set role on a {:?} node", node.kind),
                );
            }
        }
        None => {
            if node.kind == NodeKind::Instance {
                warning(
                    issues,
                    "instance-without-component",
                    path.to_string(),
                    "instance node has no component role".to_string(),
                );
            }
        }
    }
    if node.kind == NodeKind::Component {
        if !matches!(node.component, Some(ComponentRole::Component { .. })) {
            warning(
                issues,
                "component-role-missing",
                path.to_string(),
                "component node has no component role".to_string(),
            );
        }
    }
}

fn validate_prototype(
    document: &Document,
    path: &str,
    node: &Node,
    issues: &mut Vec<ValidationIssue>,
) {
    let Some(prototype) = &node.prototype else {
        return;
    };
    let mut seen = Vec::new();
    for interaction in &prototype.interactions {
        if seen.contains(&interaction.id) {
            error(
                issues,
                "duplicate-interaction",
                format!("{path}/prototype"),
                format!("interaction {} is declared twice", interaction.id),
            );
        }
        seen.push(interaction.id);
        if interaction.actions.is_empty() {
            warning(
                issues,
                "empty-interaction",
                format!("{path}/prototype"),
                format!("interaction {} has no actions", interaction.id),
            );
        }
        for action in &interaction.actions {
            match action {
                Action::NavigateTo { frame_id, .. } => validate_frame_target(
                    document,
                    path,
                    *frame_id,
                    "navigate_to",
                    issues,
                ),
                Action::OpenOverlay { frame_id, .. } => validate_frame_target(
                    document,
                    path,
                    *frame_id,
                    "open_overlay",
                    issues,
                ),
                Action::ScrollTo { .. } | Action::CloseOverlay | Action::Back => {}
                Action::SetVariable { variable_id, .. } => {
                    if document.variables.variable(*variable_id).is_none() {
                        warning(
                            issues,
                            "action-missing-variable",
                            format!("{path}/prototype"),
                            format!("action sets missing variable {variable_id}"),
                        );
                    }
                }
            }
        }
    }
}

fn validate_frame_target(
    document: &Document,
    path: &str,
    frame_id: crate::ids::NodeId,
    action: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    match document.node(frame_id) {
        None => warning(
            issues,
            "action-missing-target",
            format!("{path}/prototype"),
            format!("{action} targets missing node {frame_id}"),
        ),
        Some(target) if !matches!(target.kind, NodeKind::Frame | NodeKind::Component) => {
            warning(
                issues,
                "action-target-not-frame",
                format!("{path}/prototype"),
                format!("{action} targets a {:?}, which is not a frame", target.kind),
            );
        }
        Some(_) => {}
    }
}

fn validate_registries(document: &Document, issues: &mut Vec<ValidationIssue>) {
    for (id, meta) in &document.components.components {
        match document.node(meta.node_id) {
            None => warning(
                issues,
                "component-node-missing",
                format!("components/{id}"),
                format!("component registry points at missing node {}", meta.node_id),
            ),
            Some(node) if node.kind != NodeKind::Component => warning(
                issues,
                "component-node-kind",
                format!("components/{id}"),
                format!("registered component node is a {:?}", node.kind),
            ),
            Some(_) => {}
        }
    }
    for (id, meta) in &document.components.component_sets {
        match document.node(meta.node_id) {
            None => warning(
                issues,
                "component-set-node-missing",
                format!("component_sets/{id}"),
                format!(
                    "component set registry points at missing node {}",
                    meta.node_id
                ),
            ),
            Some(node) if node.kind != NodeKind::ComponentSet => warning(
                issues,
                "component-set-node-kind",
                format!("component_sets/{id}"),
                format!("registered component set node is a {:?}", node.kind),
            ),
            Some(_) => {}
        }
    }
    for (id, image) in &document.assets.images {
        if image.width == 0 || image.height == 0 {
            warning(
                issues,
                "empty-image",
                format!("assets/images/{id}"),
                "image asset has a zero dimension".to_string(),
            );
        }
        if image.file.is_empty() {
            error(
                issues,
                "image-without-file",
                format!("assets/images/{id}"),
                "image asset has no file name".to_string(),
            );
        }
        if !image.mime.starts_with("image/") {
            warning(
                issues,
                "image-mime",
                format!("assets/images/{id}"),
                format!("unexpected image MIME type {:?}", image.mime),
            );
        }
    }
    for (id, font) in &document.assets.fonts {
        if font.family.trim().is_empty() || font.style.trim().is_empty() {
            warning(
                issues,
                "font-metadata",
                format!("assets/fonts/{id}"),
                "embedded font is missing family or style metadata".to_string(),
            );
        }
    }
    for (id, paint) in &document.styles.paints {
        for asset_id in paint.paint.asset_ids() {
            if !document.assets.images.contains_key(&asset_id) {
                warning(
                    issues,
                    "style-missing-image",
                    format!("styles/paints/{id}"),
                    format!("paint style references missing image {asset_id}"),
                );
            }
        }
    }
    for (id, text) in &document.styles.texts {
        if text.style.font_size <= 0.0 {
            warning(
                issues,
                "style-invalid-font-size",
                format!("styles/texts/{id}"),
                "text style has a non-positive font size".to_string(),
            );
        }
    }
    for (id, effect) in &document.styles.effects {
        if effect.effects.is_empty() {
            warning(
                issues,
                "empty-effect-style",
                format!("styles/effects/{id}"),
                "effect style has no effects".to_string(),
            );
        }
    }
    for start in &document.prototype.start_frames {
        match document.node(*start) {
            None => warning(
                issues,
                "start-frame-missing",
                "prototype/start_frames".to_string(),
                format!("prototype start frame {start} does not exist"),
            ),
            Some(node) if !matches!(node.kind, NodeKind::Frame | NodeKind::Component) => warning(
                issues,
                "start-frame-not-frame",
                "prototype/start_frames".to_string(),
                format!("prototype start node {start} is a {:?}", node.kind),
            ),
            Some(_) => {}
        }
    }
    for (collection_id, mode_id) in &document.settings.active_modes {
        match document.variables.collection(*collection_id) {
            None => warning(
                issues,
                "active-mode-collection-missing",
                "settings/active_modes".to_string(),
                format!("active mode references missing collection {collection_id}"),
            ),
            Some(collection) if !collection.modes.contains(mode_id) => warning(
                issues,
                "active-mode-missing",
                "settings/active_modes".to_string(),
                format!("mode {mode_id} is not part of collection {collection_id}"),
            ),
            Some(_) => {}
        }
    }
}

fn validate_variables(document: &Document, issues: &mut Vec<ValidationIssue>) {
    for collection in &document.variables.collections {
        if !collection.modes.contains(&collection.default_mode) {
            error(
                issues,
                "collection-default-mode-missing",
                format!("variables/collections/{}", collection.id),
                "collection default mode is not one of its modes".to_string(),
            );
        }
        if collection.modes.is_empty() {
            error(
                issues,
                "collection-without-modes",
                format!("variables/collections/{}", collection.id),
                "variable collection has no modes".to_string(),
            );
        }
        if collection.name.trim().is_empty() {
            warning(
                issues,
                "collection-without-name",
                format!("variables/collections/{}", collection.id),
                "variable collection has an empty name".to_string(),
            );
        }
    }
    for (id, variable) in &document.variables.variables {
        let path = format!("variables/{id}");
        if document.variables.collection(variable.collection_id).is_none() {
            error(
                issues,
                "variable-collection-missing",
                path.clone(),
                format!(
                    "variable references missing collection {}",
                    variable.collection_id
                ),
            );
        }
        if variable.values.is_empty() {
            warning(
                issues,
                "variable-without-values",
                path.clone(),
                "variable defines no values".to_string(),
            );
        }
        for (mode_id, value) in &variable.values {
            if let Some(collection) = document.variables.collection(variable.collection_id) {
                if !collection.modes.contains(mode_id) {
                    warning(
                        issues,
                        "variable-mode-missing",
                        path.clone(),
                        format!("value for unknown mode {mode_id}"),
                    );
                }
            }
            if let VariableValue::Alias { variable_id } = value {
                if document.variables.variable(*variable_id).is_none() {
                    warning(
                        issues,
                        "variable-alias-missing",
                        path.clone(),
                        format!("alias references missing variable {variable_id}"),
                    );
                }
            }
        }
    }
}

/// Convenience: whether validation found any hard errors.
pub fn has_errors(issues: &[ValidationIssue]) -> bool {
    issues.iter().any(ValidationIssue::is_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::ids::{ComponentId, StyleId};
    use crate::node::Transform;
    use crate::paint::Fill;

    fn node_with_broken_refs() -> (Document, crate::ids::NodeId) {
        let mut document = Document::new("Validation");
        let page_id = document.active_page_id();
        let mut node = Node::rectangle("Rect", 0.0, 0.0, 10.0, 10.0);
        node.style.fills = vec![Fill {
            style_id: Some(StyleId::new_v4()),
            ..Fill::solid(Color::WHITE)
        }];
        let id = node.id;
        document
            .page_mut(page_id)
            .unwrap()
            .insert_node(node, None, None)
            .unwrap();
        (document, id)
    }

    #[test]
    fn clean_document_has_no_errors() {
        let document = Document::new("Clean");
        let issues = validate(&document);
        assert!(!has_errors(&issues), "issues: {issues:?}");
    }

    #[test]
    fn broken_references_are_warnings() {
        let (document, _) = node_with_broken_refs();
        let issues = validate(&document);
        assert!(!has_errors(&issues));
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "broken-style-reference")
        );
    }

    #[test]
    fn non_finite_transform_is_an_error() {
        let mut document = Document::new("NaN");
        let page_id = document.active_page_id();
        let mut node = Node::rectangle("Bad", 0.0, 0.0, 10.0, 10.0);
        node.transform = Transform::new(f64::NAN, 0.0, 10.0, 10.0);
        document
            .page_mut(page_id)
            .unwrap()
            .insert_node(node, None, None)
            .unwrap();
        let issues = validate(&document);
        assert!(has_errors(&issues));
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "non-finite-transform")
        );
    }

    #[test]
    fn broken_component_instance_is_reported() {
        let mut document = Document::new("Instance");
        let page_id = document.active_page_id();
        let mut node = Node::rectangle("Inst", 0.0, 0.0, 10.0, 10.0);
        node.kind = NodeKind::Instance;
        node.component = Some(ComponentRole::Instance {
            component_id: ComponentId::new_v4(),
            overrides: Vec::new(),
            swapped_component_id: None,
        });
        document
            .page_mut(page_id)
            .unwrap()
            .insert_node(node, None, None)
            .unwrap();
        let issues = validate(&document);
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "broken-instance")
        );
    }

    #[test]
    fn layout_min_above_max_is_an_error() {
        let mut document = Document::new("Layout");
        let page_id = document.active_page_id();
        let mut node = Node::frame("Frame", 0.0, 0.0, 100.0, 100.0);
        let mut layout = crate::geometry::AutoLayout::vertical(8.0, 8.0);
        layout.min_size = Some(dp_geom::Size::new(200.0, 10.0));
        layout.max_size = Some(dp_geom::Size::new(100.0, 10.0));
        node.layout = Some(layout);
        document
            .page_mut(page_id)
            .unwrap()
            .insert_node(node, None, None)
            .unwrap();
        let issues = validate(&document);
        assert!(has_errors(&issues));
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "min-greater-than-max")
        );
    }
}
