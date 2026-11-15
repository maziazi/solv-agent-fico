//! The page shell shared by every web page (`templates/layout.html`).

use super::escape::escape;

const LAYOUT: &str = include_str!("../../templates/layout.html");

/// Wraps `content` (already-escaped `<main>` markup) in the shared shell.
///
/// `title` is escaped here. The title slot is filled first and the
/// content slot last, so a marker-looking string inside `content` is never
/// expanded a second time.
pub fn page(title: &str, content: &str) -> String {
    LAYOUT
        .replacen("{{title}}", &escape(title), 1)
        .replacen("{{content}}", content, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_is_escaped_and_content_is_placed_once() {
        let html = page("<b>t</b>", "<p>body</p>");
        assert!(html.contains("<title>&lt;b&gt;t&lt;/b&gt;</title>"));
        assert_eq!(html.matches("<p>body</p>").count(), 1);
    }

    #[test]
    fn slot_markers_inside_content_are_not_expanded() {
        let html = page("title", "{{title}} {{content}}");
        assert!(html.contains("<main id=\"main\">\n{{title}} {{content}}\n</main>"));
    }

    #[test]
    fn shell_loads_only_the_local_stylesheet() {
        let html = page("t", "");
        assert!(html.contains("href=\"/static/app.css\""));
        assert!(!html.contains("http://") && !html.contains("https://"));
        assert!(!html.contains("<script"));
        assert!(!html.contains("style=\""));
    }
}
