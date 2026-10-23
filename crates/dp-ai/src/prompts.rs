//! Prompt templates and the JSON intent contract for the AI subsystem.
//!
//! Providers receive a fixed system prompt plus a task prompt. The system prompt
//! is the schema contract; every response is parsed into
//! [`crate::proposals::IntentOp`]s and validated before it can change a document.

/// JSON schema description embedded in the system prompt (kept in sync with
/// [`crate::proposals::IntentOp`]).
pub const INTENT_SCHEMA: &str = r#"Respond with a single JSON array, no prose, using only these objects:
{"op":"add_frame","name":string,"x":number,"y":number,"width":number,"height":number,"fill"?:hex}
{"op":"add_rectangle","frame"?:string,"name":string,"x":number,"y":number,"width":number,"height":number,"fill"?:hex,"radius"?:number}
{"op":"add_text","frame"?:string,"name":string,"characters":string,"x":number,"y":number,"font_size"?:number,"color"?:hex}
{"op":"set_fill","node":uuid,"color":hex}
{"op":"rename_layer","node":uuid,"name":string}
{"op":"delete_node","node":uuid}
Rules: frames must be created before children reference them by "frame"; coordinates are in pixels; colours are #rrggbb; keep layouts simple and realistic; never invent node ids."#;

/// System prompt for layout generation.
pub const SYSTEM_LAYOUT: &str = "You are a UI layout assistant for Icon Crafter. You output only valid JSON intents for the design document; you never claim to edit files.";

/// System prompt for naming/analysis helpers.
pub const SYSTEM_ASSISTANT: &str = "You are a design assistant for Icon Crafter. Answer concisely with structured JSON when asked; never invent document state.";

/// Builds the layout prompt.
pub fn layout_prompt(description: &str, width: u32, height: u32) -> String {
    format!(
        "Create a layout for this request: {description}\nTarget canvas: {width}x{height} px.\n{INTENT_SCHEMA}"
    )
}

/// Builds a layer-renaming prompt.
pub fn rename_prompt(layers: &[String]) -> String {
    format!(
        "Rename these layers to short, human-friendly names. Return one JSON object mapping the original name to the new name, e.g. {{\"Rectangle 1\": \"Hero image\"}}.\nLayers: {}",
        layers.join(", ")
    )
}

/// Builds a design-review prompt.
pub fn review_prompt(summary: &str) -> String {
    format!(
        "Review this design for usability and visual hierarchy. Reply in at most 5 short bullet points, plain text.\nDesign: {summary}"
    )
}

/// Builds an explanation prompt.
pub fn explain_prompt(summary: &str) -> String {
    format!(
        "Explain the purpose of this component to a developer joining the project. Reply in at most 4 sentences, plain text.\nComponent: {summary}"
    )
}

/// Builds a placeholder-content prompt.
pub fn content_prompt(topic: &str) -> String {
    format!(
        "Generate realistic placeholder UI copy (title, 2 short paragraphs, button labels) for: {topic}. Reply as plain lines."
    )
}

/// Builds a colour/typography suggestion prompt.
pub fn style_prompt(brand: &str) -> String {
    format!(
        "Suggest a cohesive palette (5 hex colours) and a type scale for: {brand}. Reply as plain lines with hex codes."
    )
}

/// Builds a comment-summary prompt.
pub fn comments_prompt(comments: &[String]) -> String {
    format!(
        "Summarise the open design feedback into at most 5 action items. Reply as plain lines.\nComments: {}",
        comments.join(" | ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proposals::IntentOp;

    #[test]
    fn schema_mentions_every_intent() {
        for op in ["add_frame", "add_rectangle", "add_text", "set_fill", "rename_layer", "delete_node"] {
            assert!(INTENT_SCHEMA.contains(op), "schema misses {op}");
        }
    }

    #[test]
    fn schema_examples_parse_as_intents() {
        // The two documented shapes must round-trip into the intent enum.
        let frame: IntentOp = serde_json::from_str(
            r##"{"op":"add_frame","name":"Login","x":0,"y":0,"width":390,"height":844,"fill":"#ffffff"}"##,
        )
        .unwrap();
        assert!(matches!(frame, IntentOp::AddFrame { .. }));
        let text: IntentOp = serde_json::from_str(
            r#"{"op":"add_text","frame":"Login","name":"Title","characters":"Hi","x":1,"y":2}"#,
        )
        .unwrap();
        assert!(matches!(text, IntentOp::AddText { .. }));
    }

    #[test]
    fn prompts_include_task_and_contract() {
        let layout = layout_prompt("a settings page", 1440, 900);
        assert!(layout.contains("a settings page"));
        assert!(layout.contains("add_frame"));
        let rename = rename_prompt(&["Rectangle 1".to_owned()]);
        assert!(rename.contains("Rectangle 1"));
        assert!(review_prompt("frame Login").contains("bullet points"));
        assert!(explain_prompt("frame Login").contains("developer"));
        assert!(content_prompt("pricing").contains("pricing"));
        assert!(style_prompt("fintech").contains("hex"));
        assert!(comments_prompt(&["make it bigger".to_owned()]).contains("action items"));
    }
}
