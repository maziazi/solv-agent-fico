//! The page shell shared by every web page (`templates/layout.html`).

use super::assets::css_version;
use super::escape::escape;

const LAYOUT: &str = include_str!("../../templates/layout.html");

/// What a page supplies to the shell. Everything except `title` is trusted
/// markup built by the page itself, with every dynamic value already passed
/// through [`escape`].
#[derive(Debug, Default, Clone, Copy)]
pub struct Page<'a> {
    /// Plain text. Escaped here, so a page passes it raw.
    pub title: &'a str,
    /// Extra `<head>` markup (a page-specific `<link>` or `<meta>`).
    pub head_extra: &'a str,
    /// The `<main>` content.
    pub content: &'a str,
    /// The `<footer>` content.
    pub footer: &'a str,
    /// Markup after `<footer>`, normally `<script src="…">` for a page that
    /// ships JavaScript. Empty for pages without any.
    pub scripts: &'a str,
}

/// Renders `page` into the shell.
pub fn render(page: &Page<'_>) -> String {
    let title = escape(page.title);
    let css_href = format!("/static/app.css?v={}", css_version());
    fill(
        LAYOUT,
        &[
            ("title", &title),
            ("css_href", &css_href),
            ("head_extra", page.head_extra),
            ("content", page.content),
            ("footer", page.footer),
            ("scripts", page.scripts),
        ],
    )
}

/// Fills `{{name}}` slots in a single left-to-right pass over the template.
///
/// Only the template is ever scanned for markers: an inserted value is
/// copied to the output and never looked at again, so a value that contains
/// `{{content}}` (a receipt id, say) cannot pull any other slot into itself.
fn fill(template: &str, slots: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 4096);
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        match slots.iter().find(|(slot, _)| *slot == name) {
            Some((_, value)) => out.push_str(value),
            None => {
                out.push_str("{{");
                out.push_str(name);
                out.push_str("}}");
            }
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOT_NAMES: &[&str] = &[
        "title",
        "css_href",
        "head_extra",
        "content",
        "footer",
        "scripts",
    ];

    fn hostile() -> String {
        SLOT_NAMES
            .iter()
            .map(|name| format!("{{{{{name}}}}}"))
            .collect::<Vec<_>>()
            .join("<>")
    }

    #[test]
    fn title_is_escaped_and_each_slot_is_filled_once() {
        let html = render(&Page {
            title: "<b>t</b>",
            content: "<p>body</p>",
            footer: "<p>foot</p>",
            ..Page::default()
        });
        assert!(html.contains("<title>&lt;b&gt;t&lt;/b&gt;</title>"));
        assert_eq!(html.matches("<p>body</p>").count(), 1);
        assert_eq!(html.matches("<p>foot</p>").count(), 1);
    }

    #[test]
    fn hostile_title_and_content_never_expand_each_other() {
        let evil = hostile();
        let html = render(&Page {
            title: &evil,
            content: &evil,
            footer: "<p>foot</p>",
            head_extra: "",
            scripts: "",
        });
        let title_start = html.find("<title>").unwrap() + 7;
        let title_end = html.find("</title>").unwrap();
        let title = &html[title_start..title_end];
        assert_eq!(title, escape(&evil), "the title holds exactly its own text");
        assert!(!title.contains("<p>foot</p>") && !title.contains("<main"));
        assert_eq!(html.matches("<main").count(), 1);
        assert_eq!(
            html.matches("<p>foot</p>").count(),
            1,
            "footer expanded once"
        );
        let main = &html[html.find("<main").unwrap()..html.find("</main>").unwrap()];
        assert!(
            main.contains(&evil),
            "content is inserted verbatim, once, unexpanded"
        );
    }

    #[test]
    fn unfilled_or_unknown_markers_in_the_template_are_left_visible_not_swallowed() {
        assert_eq!(fill("a {{nope}} b", &[]), "a {{nope}} b");
        assert_eq!(fill("a {{open", &[("open", "x")]), "a {{open");
        assert_eq!(fill("{{a}}{{a}}", &[("a", "{{a}}")]), "{{a}}{{a}}");
    }

    #[test]
    fn the_template_uses_exactly_the_known_slots() {
        let mut rest = LAYOUT;
        let mut seen = Vec::new();
        while let Some(start) = rest.find("{{") {
            let after = &rest[start + 2..];
            let end = after.find("}}").unwrap();
            seen.push(&after[..end]);
            rest = &after[end + 2..];
        }
        for name in &seen {
            assert!(
                SLOT_NAMES.contains(name),
                "template slot {name} has no value"
            );
        }
        for name in SLOT_NAMES {
            assert!(seen.contains(name), "slot {name} is not in the template");
        }
    }

    #[test]
    fn shell_loads_only_the_local_versioned_stylesheet_and_no_script() {
        let html = render(&Page::default());
        assert!(html.contains(&format!("href=\"/static/app.css?v={}\"", css_version())));
        assert!(!html.contains("http://") && !html.contains("https://"));
        assert!(!html.contains("<script"));
        assert!(!html.contains("style=\""));
        assert!(!html.contains("{{"));
    }
}
