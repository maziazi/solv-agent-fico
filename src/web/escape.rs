//! Contextual escaping for every dynamic value the web pages render.
//!
//! Counterparty text is attacker-controlled and ends up on a public page
//! (docs/SPEC.md §8.2). Everything goes through [`escape`] before it enters
//! markup, and the pages only place escaped values in element text — never
//! in attributes, URLs, or script.

/// Escapes `text` for an HTML text node.
///
/// Markup characters become entities. Control characters and the invisible
/// formatting characters that reorder or hide text (bidi overrides,
/// zero-width characters, line separators) become a visible `[U+XXXX]`
/// marker, so an auditor can see them instead of being steered by them.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '`' => out.push_str("&#96;"),
            '\n' | '\t' => out.push(c),
            c if is_invisible(c) => out.push_str(&format!("[U+{:04X}]", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn is_invisible(c: char) -> bool {
    c.is_control()
        || matches!(
            c as u32,
            0x061C            // Arabic letter mark
            | 0x180E          // Mongolian vowel separator
            | 0x200B..=0x200F // zero-width space/joiners, LRM, RLM
            | 0x2028..=0x202E // line/paragraph separators, bidi embeddings and overrides
            | 0x2060..=0x206F // word joiner, invisible operators, bidi isolates
            | 0xFEFF          // zero-width no-break space
            | 0xFFF9..=0xFFFB // interlinear annotation
        )
}

/// Cuts `text` to at most `max_chars` characters on a character boundary.
/// The flag says whether anything was dropped, so the page can say so.
pub fn truncate_chars(text: &str, max_chars: usize) -> (&str, bool) {
    match text.char_indices().nth(max_chars) {
        Some((byte_index, _)) => (&text[..byte_index], true),
        None => (text, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_characters_become_entities() {
        assert_eq!(
            escape(r#"<script>alert("x")</script> & 'y' `z`"#),
            "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt; &amp; &#39;y&#39; &#96;z&#96;"
        );
    }

    #[test]
    fn ampersand_is_escaped_so_entities_cannot_be_forged() {
        assert_eq!(escape("&lt;"), "&amp;lt;");
    }

    #[test]
    fn bidi_and_zero_width_characters_are_made_visible() {
        let hostile = "pay\u{202E}1000\u{202C}\u{200B}\u{FEFF}\u{2028}\u{0007}";
        assert_eq!(
            escape(hostile),
            "pay[U+202E]1000[U+202C][U+200B][U+FEFF][U+2028][U+0007]"
        );
    }

    #[test]
    fn newline_tab_and_ordinary_unicode_are_untouched() {
        assert_eq!(escape("a\nb\tc é 日本 😀"), "a\nb\tc é 日本 😀");
    }

    #[test]
    fn truncation_respects_character_boundaries() {
        assert_eq!(truncate_chars("héllo", 2), ("hé", true));
        assert_eq!(truncate_chars("héllo", 5), ("héllo", false));
        assert_eq!(truncate_chars("😀😀😀", 2), ("😀😀", true));
        assert_eq!(truncate_chars("", 0), ("", false));
    }
}
