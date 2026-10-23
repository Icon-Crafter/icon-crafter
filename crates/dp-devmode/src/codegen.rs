//! Deterministic developer code generation.
//!
//! Every generator walks the inspected selection in paint order and emits a
//! visual approximation of the design. The output is intentionally plain and
//! stable (no timestamps, no random ids) so it can be diffed in reviews.
//!
//! Honesty rules baked into this module: generated code is labelled as an
//! approximation, unsupported features produce explicit warnings and
//! `requires_manual_adaptation` is set whenever the output cannot be a faithful
//! implementation (vectors, images, booleans, blur effects, auto-layout).

use std::collections::HashSet;
use std::fmt::Write as _;

use dp_layout::LayoutMap;
use dp_model::{Color, Document, NodeId, PageId};
use serde::{Deserialize, Serialize};

use crate::inspect::{EffectInfo, NodeInspect, PaintInfo, inspect};

/// Code generator target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeTarget {
    /// Plain CSS rules.
    Css,
    /// React function component + TypeScript.
    ReactTypeScript,
    /// SwiftUI view.
    SwiftUi,
    /// Jetpack Compose (Kotlin).
    Compose,
}

impl CodeTarget {
    /// File extension hint.
    pub fn extension(self) -> &'static str {
        match self {
            CodeTarget::Css => "css",
            CodeTarget::ReactTypeScript => "tsx",
            CodeTarget::SwiftUi => "swift",
            CodeTarget::Compose => "kt",
        }
    }

    /// Language label shown in the UI.
    pub fn language(self) -> &'static str {
        match self {
            CodeTarget::Css => "css",
            CodeTarget::ReactTypeScript => "typescript",
            CodeTarget::SwiftUi => "swift",
            CodeTarget::Compose => "kotlin",
        }
    }
}

/// Code generation options.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CodegenOptions {
    /// Output target.
    pub target: CodeTarget,
    /// Decimal places for emitted numbers.
    pub precision: u32,
    /// Include explanatory comments.
    pub include_comments: bool,
    /// Emit a wrapper container sized to the selection.
    pub embed_wrapper: bool,
}

impl CodegenOptions {
    /// Options for a target with sensible defaults.
    pub fn new(target: CodeTarget) -> Self {
        Self {
            target,
            precision: 2,
            include_comments: true,
            embed_wrapper: true,
        }
    }
}

/// Generated code and its caveats.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeneratedCode {
    /// Target that produced the code.
    pub target: CodeTarget,
    /// Language label.
    pub language: String,
    /// Suggested file extension.
    pub extension: String,
    /// Generated source.
    pub code: String,
    /// Things the developer must handle manually.
    pub warnings: Vec<String>,
    /// Whether the result is a visual approximation needing adaptation.
    pub requires_manual_adaptation: bool,
    /// Number of nodes represented.
    pub node_count: usize,
}

/// Code generation error.
#[derive(Debug, thiserror::Error)]
pub enum CodegenError {
    /// The selection is empty or contains no nodes of the selected page.
    #[error("select at least one node on the current page to generate code")]
    EmptySelection,
    /// The layout cache does not contain the page.
    #[error("the page has not been laid out; call layout before generating code")]
    MissingLayout,
}

/// Generates code for `ids` in paint order.
pub fn generate(
    document: &Document,
    layout: &LayoutMap,
    ids: &[NodeId],
    options: &CodegenOptions,
) -> Result<GeneratedCode, CodegenError> {
    if ids.is_empty() {
        return Err(CodegenError::EmptySelection);
    }
    let Some((page, _)) = ids.iter().find_map(|id| document.locate_node(*id)) else {
        return Err(CodegenError::EmptySelection);
    };
    let page_id = page.id;
    let order = collect_subtree(document, page_id, ids);
    let snapshot = inspect(document, layout, &order);
    if snapshot.nodes.is_empty() {
        return Err(CodegenError::EmptySelection);
    }
    let nodes: Vec<&NodeInspect> = snapshot.nodes.iter().collect();
    let by_id: std::collections::HashMap<&str, &NodeInspect> = snapshot
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();

    let mut warnings = snapshot.warnings.clone();
    for node in &snapshot.nodes {
        collect_warnings(node, &mut warnings);
    }
    dedupe(&mut warnings);

    let precision = options.precision.clamp(0, 4);
    let code = match options.target {
        CodeTarget::Css => css(document, &nodes, &by_id, precision, options),
        CodeTarget::ReactTypeScript => react(document, &nodes, &by_id, precision, options),
        CodeTarget::SwiftUi => swiftui(document, &nodes, &by_id, precision, options),
        CodeTarget::Compose => compose(document, &nodes, &by_id, precision, options),
    };
    let mut requires_manual_adaptation = !warnings.is_empty();
    match options.target {
        CodeTarget::Css | CodeTarget::ReactTypeScript => {
            requires_manual_adaptation |= nodes.iter().any(|node| node.text.is_none());
        }
        CodeTarget::SwiftUi | CodeTarget::Compose => {
            requires_manual_adaptation |= true;
            warnings.push(
                "SwiftUI/Compose output uses fixed frames with manual offsets; real layouts need responsive containers."
                    .to_owned(),
            );
        }
    }
    dedupe(&mut warnings);
    Ok(GeneratedCode {
        target: options.target,
        language: options.target.language().to_owned(),
        extension: options.target.extension().to_owned(),
        code,
        warnings,
        requires_manual_adaptation,
        node_count: nodes.len(),
    })
}

fn collect_subtree(document: &Document, page: PageId, ids: &[NodeId]) -> Vec<NodeId> {
    let Some(page) = document.page(page) else {
        return Vec::new();
    };
    let selected: HashSet<NodeId> = ids.iter().copied().collect();
    // Skip ids already covered by another selected ancestor.
    let mut roots = Vec::new();
    for id in ids {
        let mut covered = false;
        let mut parent = page.nodes.get(id).and_then(|node| node.parent);
        let mut guard = 0;
        while let Some(parent_id) = parent {
            guard += 1;
            if guard > 256 {
                break;
            }
            if selected.contains(&parent_id) {
                covered = true;
                break;
            }
            parent = page.nodes.get(&parent_id).and_then(|node| node.parent);
        }
        if !covered {
            roots.push(*id);
        }
    }
    let mut order = Vec::new();
    let mut seen = HashSet::new();
    for root in roots {
        push_preorder(page, root, &mut order, &mut seen);
    }
    order
}

fn push_preorder(
    page: &dp_model::Page,
    id: NodeId,
    order: &mut Vec<NodeId>,
    seen: &mut HashSet<NodeId>,
) {
    if !seen.insert(id) {
        return;
    }
    order.push(id);
    if let Some(node) = page.nodes.get(&id) {
        for child in &node.children {
            push_preorder(page, *child, order, seen);
        }
    }
}

fn collect_warnings(node: &NodeInspect, warnings: &mut Vec<String>) {
    match node.kind.as_str() {
        "vector" => warnings.push(format!(
            "\"{}\": vector geometry is not emitted as DOM; export it as SVG and reference the asset.",
            node.name
        )),
        "boolean" => warnings.push(format!(
            "\"{}\": boolean geometry must be exported as SVG (children are real editable operands).",
            node.name
        )),
        "image" => warnings.push(format!(
            "\"{}\": image fills/assets are not embedded by the generator.",
            node.name
        )),
        "sticky" | "connector" => warnings.push(format!(
            "\"{}\": whiteboard object has no web equivalent; implement it semantically.",
            node.name
        )),
        _ => {}
    }
    if node.fills.iter().any(|fill| fill.kind != "solid") {
        warnings.push(format!(
            "\"{}\": gradients/multiple fills are approximated by the first solid/gradient paint.",
            node.name
        ));
    }
    if node.layout_direction.is_some() {
        warnings.push(format!(
            "\"{}\": auto layout is flattened into absolute offsets; rebuild the layout with the target framework's flex/stack layout.",
            node.name
        ));
    }
    if node
        .effects
        .iter()
        .any(|effect| effect.blur > 0.0 && node.blend != "normal")
    {
        warnings.push(format!(
            "\"{}\": blend modes with effects need manual compositing in the target platform.",
            node.name
        ));
    }
}

fn dedupe(warnings: &mut Vec<String>) {
    let mut seen = HashSet::new();
    warnings.retain(|warning| seen.insert(warning.clone()));
}

fn number(value: f64, precision: u32) -> String {
    let mut text = format!("{:.p$}", value, p = precision as usize);
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" { "0".to_owned() } else { text }
}

fn px(value: f64, precision: u32) -> String {
    format!("{}px", number(value, precision))
}

fn css_color(color: Color) -> String {
    if color.a >= 0.999 {
        color.to_hex()
    } else {
        format!(
            "rgba({},{},{},{})",
            (color.r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (color.g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (color.b.clamp(0.0, 1.0) * 255.0).round() as u8,
            number(color.a, 3)
        )
    }
}

fn primary_fill(node: &NodeInspect) -> Option<&PaintInfo> {
    node.fills.first()
}

fn background_css(node: &NodeInspect, _precision: u32) -> Option<String> {
    let fill = primary_fill(node)?;
    match fill.kind.as_str() {
        "solid" => fill.color.map(css_color),
        "linear" | "radial" => {
            if fill.stops.is_empty() {
                return None;
            }
            let mut stops = String::new();
            for stop in &fill.stops {
                let _ = write!(
                    stops,
                    "{}{}%, ",
                    css_color(stop.color),
                    number(stop.position * 100.0, 1)
                );
            }
            let stops = stops.trim_end_matches(", ");
            if fill.kind == "linear" {
                Some(format!("linear-gradient(180deg, {stops})"))
            } else {
                Some(format!("radial-gradient(circle, {stops})"))
            }
        }
        _ => None,
    }
}

fn border_css(node: &NodeInspect, precision: u32) -> Option<String> {
    let stroke = node.strokes.first()?;
    let color = stroke.paint.color?;
    Some(format!(
        "{} solid {}",
        px(stroke.weight, precision),
        css_color(color)
    ))
}

fn radii_css(node: &NodeInspect, precision: u32) -> String {
    let radii = node.corner_radii;
    if radii.iter().all(|radius| *radius <= 0.0) {
        return "0".to_owned();
    }
    radii
        .iter()
        .map(|radius| px(*radius, precision))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shadow_css(effects: &[EffectInfo], precision: u32) -> Option<String> {
    let shadows: Vec<String> = effects
        .iter()
        .filter(|effect| effect.kind == "drop_shadow")
        .filter_map(|effect| {
            let color = css_color(effect.color?);
            let offset = effect.offset.unwrap_or([0.0, 0.0]);
            Some(format!(
                "{} {} {} {} {}",
                px(offset[0], precision),
                px(offset[1], precision),
                px(effect.blur, precision),
                px(effect.spread, precision),
                color
            ))
        })
        .collect();
    if shadows.is_empty() {
        None
    } else {
        Some(shadows.join(", "))
    }
}

fn rotate_css(node: &NodeInspect, precision: u32) -> Option<String> {
    if node.rotation.abs() < 0.01 {
        return None;
    }
    Some(format!("rotate({}deg)", number(node.rotation, precision)))
}

fn node_style_props(
    node: &NodeInspect,
    precision: u32,
    position: (f64, f64),
    size: (f64, f64),
) -> Vec<(String, String)> {
    let mut props = vec![
        ("position".to_owned(), "absolute".to_owned()),
        ("left".to_owned(), px(position.0, precision)),
        ("top".to_owned(), px(position.1, precision)),
        ("width".to_owned(), px(size.0, precision)),
        ("height".to_owned(), px(size.1, precision)),
    ];
    if node.opacity < 0.999 {
        props.push(("opacity".to_owned(), number(node.opacity, 3)));
    }
    if let Some(background) = background_css(node, precision) {
        props.push(("background".to_owned(), background));
    }
    if let Some(border) = border_css(node, precision) {
        props.push(("border".to_owned(), border));
    }
    let radii = radii_css(node, precision);
    if radii != "0" {
        props.push(("border-radius".to_owned(), radii));
    }
    if let Some(shadow) = shadow_css(&node.effects, precision) {
        props.push(("box-shadow".to_owned(), shadow));
    }
    if let Some(rotate) = rotate_css(node, precision) {
        props.push(("transform".to_owned(), rotate));
    }
    if node.blend != "normal" && node.blend != "pass_through" {
        props.push(("mix-blend-mode".to_owned(), node.blend.replace('_', "-")));
    }
    if let Some(text) = &node.text {
        props.push((
            "font-family".to_owned(),
            format!("\"{}\"", text.font_family),
        ));
        props.push(("font-size".to_owned(), px(text.font_size, precision)));
        props.push(("font-weight".to_owned(), text.font_weight.to_string()));
        if text.italic {
            props.push(("font-style".to_owned(), "italic".to_owned()));
        }
        props.push(("line-height".to_owned(), px(text.line_height, precision)));
        if text.letter_spacing.abs() > 0.001 {
            props.push((
                "letter-spacing".to_owned(),
                px(text.letter_spacing, precision),
            ));
        }
        props.push(("text-align".to_owned(), text.align.replace('_', "-")));
        if let Some(color) = text.color {
            props.push(("color".to_owned(), css_color(color)));
        }
    }
    props
}

fn class_name(name: &str, index: usize) -> String {
    let mut slug = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        format!("node-{index}")
    } else {
        format!("{slug}-{index}")
    }
}

fn css(
    document: &Document,
    nodes: &[&NodeInspect],
    by_id: &std::collections::HashMap<&str, &NodeInspect>,
    precision: u32,
    options: &CodegenOptions,
) -> String {
    let _ = (document, by_id);
    let mut out = String::new();
    if options.include_comments {
        out.push_str(
            "/* Generated by Icon Crafter Dev Mode - visual approximation, review before use. */\n",
        );
        out.push_str("/* Absolute positions are relative to the selection container. */\n\n");
    }
    let root = nodes[0];
    let origin = root.bounds;
    if options.embed_wrapper {
        let _ = writeln!(out, ".iconcrafter-selection {{");
        let _ = writeln!(out, "  position: relative;");
        let _ = writeln!(out, "  width: {};", px(origin[2], precision));
        let _ = writeln!(out, "  height: {};", px(origin[3], precision));
        let _ = writeln!(out, "}}\n");
    }
    for (index, node) in nodes.iter().enumerate() {
        let class = class_name(&node.name, index);
        let local = if index == 0 {
            (0.0, 0.0)
        } else {
            (node.bounds[0] - origin[0], node.bounds[1] - origin[1])
        };
        let size = (node.bounds[2], node.bounds[3]);
        let props = node_style_props(node, precision, local, size);
        let _ = writeln!(out, ".{class} {{");
        for (key, value) in &props {
            let _ = writeln!(out, "  {key}: {value};");
        }
        let _ = writeln!(out, "}}");
        if let Some(text) = &node.text {
            let _ = writeln!(out, "/* .{class} content: {:?} */\n", text.characters);
        } else {
            out.push('\n');
        }
    }
    out
}

fn react(
    document: &Document,
    nodes: &[&NodeInspect],
    by_id: &std::collections::HashMap<&str, &NodeInspect>,
    precision: u32,
    options: &CodegenOptions,
) -> String {
    let _ = document;
    let mut out = String::new();
    if options.include_comments {
        out.push_str(
            "// Generated by Icon Crafter Dev Mode - visual approximation, review before use.\n",
        );
        out.push_str(
            "// Types are inferred; wire up real state, events and responsive layout manually.\n\n",
        );
    }
    let root = nodes[0];
    let component = component_name(&root.name);
    let _ = writeln!(out, "import type {{ CSSProperties }} from \"react\";\n");
    let _ = writeln!(out, "export function {component}(): React.JSX.Element {{");
    let mut child_map: std::collections::HashMap<Option<&str>, Vec<&NodeInspect>> =
        std::collections::HashMap::new();
    for node in nodes {
        child_map
            .entry(node.parent.as_deref())
            .or_default()
            .push(node);
    }
    let root_id = nodes
        .iter()
        .find(|node| {
            node.parent.is_none() || !by_id.contains_key(node.parent.as_deref().unwrap_or_default())
        })
        .map(|node| node.id.as_str())
        .unwrap_or(root.id.as_str());
    render_react_node(&mut out, root_id, &child_map, by_id, precision, 1);
    let _ = writeln!(out, "}}");
    out
}

#[allow(clippy::too_many_arguments)]
fn render_react_node(
    out: &mut String,
    id: &str,
    child_map: &std::collections::HashMap<Option<&str>, Vec<&NodeInspect>>,
    by_id: &std::collections::HashMap<&str, &NodeInspect>,
    precision: u32,
    depth: usize,
) {
    let Some(node) = by_id.get(id) else {
        return;
    };
    let indent = "  ".repeat(depth);
    let position = if node.parent.is_none() {
        (0.0, 0.0)
    } else {
        (node.local_position[0], node.local_position[1])
    };
    let size = (node.bounds[2], node.bounds[3]);
    let props = node_style_props(node, precision, position, size);
    let _ = writeln!(out, "{indent}<div");
    let _ = writeln!(out, "{indent}  style={{{{");
    for (key, value) in &props {
        let camel = to_camel_case(key);
        let literal = if key == "width"
            || key == "height"
            || key == "left"
            || key == "top"
            || key.ends_with("-size")
            || key == "letter-spacing"
            || key == "line-height"
        {
            value.trim_end_matches("px").to_owned()
        } else if key == "position" || key == "font-style" || key == "text-align" {
            format!("\"{value}\"")
        } else if key == "font-weight" || key == "opacity" {
            value.to_owned()
        } else {
            format!("\"{value}\"")
        };
        let _ = writeln!(out, "{indent}    {camel}: {literal},");
    }
    let _ = writeln!(out, "{indent}  }}}}");
    let _ = writeln!(
        out,
        "{indent}  aria-label=\"{}\"",
        node.name.replace('"', "'")
    );
    if let Some(text) = &node.text {
        let content = text_characters(&text.characters);
        let _ = writeln!(out, "{indent}>");
        let _ = writeln!(out, "{indent}  {content}");
        let _ = writeln!(out, "{indent}</div>");
        return;
    }
    let children = child_map.get(&Some(id)).cloned().unwrap_or_default();
    if children.is_empty() {
        let _ = writeln!(out, "{indent} />");
        return;
    }
    let _ = writeln!(out, "{indent}>");
    for child in children {
        render_react_node(
            out,
            child.id.as_str(),
            child_map,
            by_id,
            precision,
            depth + 1,
        );
    }
    let _ = writeln!(out, "{indent}</div>");
}

fn text_characters(text: &str) -> String {
    if text.contains(['\n', '<', '>', '&', '{', '}']) {
        format!("{{{:?}}}", text)
    } else {
        text.to_owned()
    }
}

fn to_camel_case(css: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for character in css.chars() {
        if character == '-' {
            upper = true;
        } else if upper {
            out.extend(character.to_uppercase());
            upper = false;
        } else {
            out.push(character);
        }
    }
    out
}

fn component_name(name: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if upper {
                out.extend(character.to_uppercase());
                upper = false;
            } else {
                out.push(character);
            }
        } else {
            upper = true;
        }
    }
    if out.is_empty()
        || out
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit())
    {
        format!("Generated{out}View")
    } else {
        format!("{out}View")
    }
}

fn swiftui(
    document: &Document,
    nodes: &[&NodeInspect],
    _by_id: &std::collections::HashMap<&str, &NodeInspect>,
    precision: u32,
    options: &CodegenOptions,
) -> String {
    let _ = document;
    let mut out = String::new();
    if options.include_comments {
        out.push_str("// Generated by Icon Crafter Dev Mode - visual approximation for SwiftUI.\n");
        out.push_str(
            "// Fixed frames + offsets: adapt to real layout containers and dynamic type.\n\n",
        );
    }
    let root = nodes[0];
    let name = component_name(&root.name);
    let _ = writeln!(out, "import SwiftUI\n");
    let _ = writeln!(out, "struct {name}: View {{");
    let _ = writeln!(out, "  var body: some View {{");
    let _ = writeln!(out, "    ZStack(alignment: .topLeading) {{");
    for node in nodes {
        let x = node.bounds[0] - root.bounds[0];
        let y = node.bounds[1] - root.bounds[1];
        let width = node.bounds[2];
        let height = node.bounds[3];
        let radius = node.corner_radii.iter().copied().fold(0.0, f64::max);
        if let Some(text) = &node.text {
            let color = text
                .color
                .map(|color| format!(".foregroundStyle(Color(hex: \"{}\"))", color.to_hex()))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "      Text({:?}).font(.system(size: {}, weight: .{})){color}",
                text.characters,
                number(text.font_size, precision),
                swift_weight(text.font_weight)
            );
            let _ = writeln!(
                out,
                "        .frame(width: {}, height: {}, alignment: .leading)",
                number(width, precision),
                number(height, precision)
            );
            let _ = writeln!(
                out,
                "        .offset(x: {}, y: {})",
                number(x, precision),
                number(y, precision)
            );
        } else {
            let fill = primary_fill(node)
                .and_then(|fill| fill.color)
                .map(|color| format!("Color(hex: \"{}\")", color.to_hex()))
                .unwrap_or_else(|| ".clear".to_owned());
            let _ = writeln!(
                out,
                "      RoundedRectangle(cornerRadius: {})",
                number(radius, precision)
            );
            let _ = writeln!(
                out,
                "        .fill({fill}).frame(width: {}, height: {})",
                number(width, precision),
                number(height, precision)
            );
            let _ = writeln!(
                out,
                "        .offset(x: {}, y: {})",
                number(x, precision),
                number(y, precision)
            );
        }
    }
    let _ = writeln!(
        out,
        "    }}.frame(width: {}, height: {}, alignment: .topLeading)",
        number(root.bounds[2], precision),
        number(root.bounds[3], precision)
    );
    let _ = writeln!(out, "  }}");
    let _ = writeln!(out, "}}");
    out
}

fn swift_weight(weight: u16) -> &'static str {
    match weight {
        0..=399 => "regular",
        400..=499 => "regular",
        500..=599 => "medium",
        600..=699 => "semibold",
        700..=799 => "bold",
        _ => "heavy",
    }
}

fn compose(
    document: &Document,
    nodes: &[&NodeInspect],
    by_id: &std::collections::HashMap<&str, &NodeInspect>,
    precision: u32,
    options: &CodegenOptions,
) -> String {
    let _ = (document, by_id);
    let mut out = String::new();
    if options.include_comments {
        out.push_str(
            "// Generated by Icon Crafter Dev Mode - visual approximation for Jetpack Compose.\n",
        );
        out.push_str(
            "// Absolute offsets: replace with real layout primitives for responsive screens.\n\n",
        );
    }
    let root = nodes[0];
    let name = component_name(&root.name);
    out.push_str("import androidx.compose.foundation.background\n");
    out.push_str("import androidx.compose.foundation.layout.*\n");
    out.push_str("import androidx.compose.foundation.shape.RoundedCornerShape\n");
    out.push_str("import androidx.compose.material3.Text\n");
    out.push_str("import androidx.compose.runtime.Composable\n");
    out.push_str("import androidx.compose.ui.Modifier\n");
    out.push_str("import androidx.compose.ui.graphics.Color\n");
    out.push_str("import androidx.compose.ui.unit.dp\n");
    out.push_str("import androidx.compose.ui.unit.sp\n\n");
    let _ = writeln!(out, "@Composable");
    let _ = writeln!(out, "fun {name}() {{");
    let _ = writeln!(
        out,
        "  Box(Modifier.size({}.dp)) {{",
        number(root.bounds[2], precision)
    );
    for node in nodes {
        let x = node.bounds[0] - root.bounds[0];
        let y = node.bounds[1] - root.bounds[1];
        let width = number(node.bounds[2], precision);
        let height = number(node.bounds[3], precision);
        if let Some(text) = &node.text {
            let color = text
                .color
                .map(|color| format!("Color(0x{:08X})", argb(color)))
                .unwrap_or_else(|| "Color.Unspecified".to_owned());
            let _ = writeln!(
                out,
                "    Text({:?}, modifier = Modifier.offset(x = {}.dp, y = {}.dp).size({}.dp, {}.dp), color = {color}, fontSize = {}.sp)",
                text.characters,
                number(x, precision),
                number(y, precision),
                width,
                height,
                number(text.font_size, precision)
            );
        } else {
            let color = primary_fill(node)
                .and_then(|fill| fill.color)
                .map(|color| format!("Color(0x{:08X})", argb(color)))
                .unwrap_or_else(|| "Color.Transparent".to_owned());
            let radius = node.corner_radii.iter().copied().fold(0.0, f64::max);
            let _ = writeln!(
                out,
                "    Box(Modifier.offset(x = {}.dp, y = {}.dp).size({width}.dp, {height}.dp).background({color}, RoundedCornerShape({}.dp)))",
                number(x, precision),
                number(y, precision),
                number(radius, precision)
            );
        }
    }
    let _ = writeln!(out, "  }}");
    let _ = writeln!(out, "}}");
    out
}

fn argb(color: Color) -> u32 {
    let a = (color.a.clamp(0.0, 1.0) * 255.0).round() as u32;
    let r = (color.r.clamp(0.0, 1.0) * 255.0).round() as u32;
    let g = (color.g.clamp(0.0, 1.0) * 255.0).round() as u32;
    let b = (color.b.clamp(0.0, 1.0) * 255.0).round() as u32;
    (a << 24) | (r << 16) | (g << 8) | b
}

#[cfg(test)]
mod tests {
    use super::*;
    use dp_layout::LayoutEngine;
    use dp_model::{Fill, Node};

    fn document() -> Document {
        let mut document = Document::new("Codegen");
        let page_id = document.active_page_id();
        let page = document.page_mut(page_id).expect("page");
        let mut card = Node::frame("Card", 0.0, 0.0, 240.0, 120.0);
        card.style.fills = vec![Fill::solid(Color::from_hex("#ff6b4a").unwrap())];
        card.style.corner_radii = dp_geom::CornerRadii::uniform(8.0);
        let card_id = card.id;
        page.insert_node(card, None, None).unwrap();
        let mut title = Node::text("Title", "Hello", 16.0, 20.0);
        title.style.fills = vec![Fill::solid(Color::WHITE)];
        if let Some(content) = title.text.as_mut() {
            content.style.font_size = 18.0;
        }
        page.insert_node(title, Some(card_id), None).unwrap();
        document
    }

    fn layout(document: &Document) -> dp_layout::LayoutMap {
        LayoutEngine::new().compute_page(document.active_page())
    }

    #[test]
    fn css_generates_absolute_container_and_rules() {
        let document = document();
        let layout = layout(&document);
        let ids: Vec<NodeId> = document.active_page().roots.clone();
        let options = CodegenOptions::new(CodeTarget::Css);
        let generated = generate(&document, &layout, &ids, &options).unwrap();
        assert_eq!(generated.extension, "css");
        assert!(generated.code.contains(".iconcrafter-selection"));
        assert!(generated.code.contains("position: absolute"));
        assert!(generated.code.contains("#ff6b4a"));
        assert!(!generated.code.contains("background: linear-gradient"));
        assert!(generated.node_count >= 1);
    }

    #[test]
    fn react_emits_typed_component() {
        let document = document();
        let layout = layout(&document);
        let ids: Vec<NodeId> = document.active_page().roots.clone();
        let options = CodegenOptions::new(CodeTarget::ReactTypeScript);
        let generated = generate(&document, &layout, &ids, &options).unwrap();
        assert!(generated.code.contains("export function CardView()"));
        assert!(generated.code.contains("React.JSX.Element"));
        assert!(generated.code.contains("Hello"));
        assert!(generated.code.contains("borderRadius"));
        assert_eq!(generated.extension, "tsx");
    }

    #[test]
    fn swiftui_and_compose_emit_platform_primitives() {
        let document = document();
        let layout = layout(&document);
        let ids: Vec<NodeId> = document.active_page().roots.clone();
        let swift = generate(
            &document,
            &layout,
            &ids,
            &CodegenOptions::new(CodeTarget::SwiftUi),
        )
        .unwrap();
        assert!(swift.code.contains("ZStack(alignment: .topLeading)"));
        assert!(swift.code.contains("RoundedRectangle"));
        assert!(swift.requires_manual_adaptation);

        let compose = generate(
            &document,
            &layout,
            &ids,
            &CodegenOptions::new(CodeTarget::Compose),
        )
        .unwrap();
        assert!(compose.code.contains("@Composable"));
        assert!(compose.code.contains("RoundedCornerShape"));
        assert!(compose.code.contains("Text("));
    }

    #[test]
    fn output_is_deterministic() {
        let document = document();
        let layout = layout(&document);
        let ids: Vec<NodeId> = document.active_page().roots.clone();
        let options = CodegenOptions::new(CodeTarget::ReactTypeScript);
        let first = generate(&document, &layout, &ids, &options).unwrap();
        let second = generate(&document, &layout, &ids, &options).unwrap();
        assert_eq!(first.code, second.code);
        assert_eq!(first.warnings, second.warnings);
    }

    #[test]
    fn vectors_are_flagged_as_manual() {
        let mut document = document();
        let page_id = document.active_page_id();
        let page = document.page_mut(page_id).expect("page");
        let node = Node::new(
            dp_model::NodeKind::Vector,
            "Sparkle",
            dp_model::Transform::new(10.0, 10.0, 20.0, 20.0),
        );
        page.insert_node(node, None, None).unwrap();
        let layout = layout(&document);
        let ids: Vec<NodeId> = document.active_page().roots.clone();
        let generated = generate(
            &document,
            &layout,
            &ids,
            &CodegenOptions::new(CodeTarget::Css),
        )
        .unwrap();
        assert!(
            generated
                .warnings
                .iter()
                .any(|warning| warning.contains("vector geometry"))
        );
        assert!(generated.requires_manual_adaptation);
    }

    #[test]
    fn empty_selection_is_an_error() {
        let document = document();
        let layout = layout(&document);
        let result = generate(
            &document,
            &layout,
            &[],
            &CodegenOptions::new(CodeTarget::Css),
        );
        assert!(matches!(result, Err(CodegenError::EmptySelection)));
    }
}
