//! Publishing: render a self-contained HTML export bundle.

use dp_model::Document;
use serde_json::Value;

/// Render a self-contained static HTML page for a published document.
///
/// The bundle embeds the canonical document JSON and a manifest, and is a
/// static preview only: no scripts run in the exported page.
pub fn render_export_html(
    document: &Document,
    document_json: &Value,
    slug: &str,
    file_name: &str,
) -> Result<String, serde_json::Error> {
    let embedded = serde_json::to_string(document_json)?;
    let node_count = document.node_count();
    let page_count = document.pages.len();
    let revision = document.revision;
    let escaped_name = html_escape(file_name);
    let escaped_slug = html_escape(slug);
    Ok(format!(
        "<!DOCTYPE html>\n\
<html lang=\"en\">\n\
<head>\n\
<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<meta name=\"generator\" content=\"iconcrafter-server\">\n\
<meta name=\"iconcrafter:slug\" content=\"{escaped_slug}\">\n\
<title>{escaped_name} · Icon Crafter</title>\n\
<style>\n\
  :root {{ color-scheme: light dark; }}\n\
  body {{ margin: 0; font-family: ui-sans-serif, system-ui, sans-serif; background: #0f1115; color: #e8eaf0; }}\n\
  main {{ max-width: 960px; margin: 0 auto; padding: 48px 24px 64px; }}\n\
  h1 {{ font-size: 28px; margin: 0 0 8px; }}\n\
  p {{ color: #a3a9b8; line-height: 1.6; }}\n\
  .card {{ border: 1px solid #2a2f3a; border-radius: 12px; padding: 20px; margin-top: 24px; background: #151922; }}\n\
  code {{ background: #1e2430; border-radius: 4px; padding: 1px 5px; font-size: 13px; }}\n\
  ul {{ padding-left: 20px; color: #a3a9b8; }}\n\
</style>\n\
</head>\n\
<body>\n\
<main>\n\
  <h1>{escaped_name}</h1>\n\
  <p>Static export published by Icon Crafter. This bundle contains the canonical document data; \
it does not run editor code.</p>\n\
  <div class=\"card\">\n\
    <p><strong>Manifest</strong></p>\n\
    <ul>\n\
      <li>slug: <code>{escaped_slug}</code></li>\n\
      <li>pages: <code>{page_count}</code></li>\n\
      <li>nodes: <code>{node_count}</code></li>\n\
      <li>revision: <code>{revision}</code></li>\n\
      <li>format: <code>iconcrafter-publish/v1</code></li>\n\
    </ul>\n\
  </div>\n\
</main>\n\
<script type=\"application/json\" id=\"iconcrafter-document\">{embedded}</script>\n\
</body>\n\
</html>\n"
    ))
}

/// Escape text that is interpolated into the HTML body.
fn html_escape(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_escaping_is_applied() {
        assert_eq!(
            html_escape("<script>\"x\" & 'y'</script>"),
            "&lt;script&gt;&quot;x&quot; &amp; &#39;y&#39;&lt;/script&gt;"
        );
    }

    #[test]
    fn render_embeds_document_json() {
        let document = Document::new("Landing page");
        let json = document.to_json_value().expect("json");
        let html =
            render_export_html(&document, &json, "landing-page", "Landing page").expect("html");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("iconcrafter-document"));
        assert!(html.contains("landing-page"));
        assert!(html.contains("Landing page"));
        assert!(html.contains("iconcrafter-publish/v1"));
        // The embedded JSON is parseable content, not raw HTML injection.
        assert!(!html.contains("</script><script>"));
    }

    #[test]
    fn render_escapes_names() {
        let document = Document::new("safe");
        let json = document.to_json_value().expect("json");
        let html = render_export_html(&document, &json, "safe", "<b>unsafe</b>").expect("html");
        assert!(html.contains("&lt;b&gt;unsafe&lt;/b&gt;"));
        assert!(!html.contains("<b>unsafe</b>"));
    }
}
