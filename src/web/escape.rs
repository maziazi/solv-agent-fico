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
            0x00AD            // soft hyphen
            | 0x034F          // combining grapheme joiner
            | 0x061C          // Arabic letter mark
            | 0x115F..=0x1160 // Hangul choseong/jungseong fillers
            | 0x17B4..=0x17B5 // Khmer inherent vowels
            | 0x180B..=0x180E // Mongolian free variation selectors and vowel separator
            | 0x200B..=0x200F // zero-width space/joiners, LRM, RLM
            | 0x2028..=0x202E // line/paragraph separators, bidi embeddings and overrides
            | 0x2060..=0x206F // word joiner, invisible operators, bidi isolates
            | 0x3164          // Hangul filler
            | 0xFE00..=0xFE0F // variation selectors
            | 0xFEFF          // zero-width no-break space
            | 0xFFA0          // halfwidth Hangul filler
            | 0xFFF9..=0xFFFB // interlinear annotation
            | 0x1D173..=0x1D17A // musical symbol invisible formatting
            | 0xE0000..=0xE007F // tag characters: invisible text smuggling
            | 0xE0100..=0xE01EF // variation selectors supplement
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

    // Expected markers are written out by hand, not derived from the
    // implementation's own ranges.
    #[test]
    fn each_invisible_range_is_marked_at_its_edges() {
        for (input, expected) in [
            ("\u{00AD}", "[U+00AD]"),
            ("\u{034F}", "[U+034F]"),
            ("\u{061C}", "[U+061C]"),
            ("\u{115F}", "[U+115F]"),
            ("\u{1160}", "[U+1160]"),
            ("\u{17B4}", "[U+17B4]"),
            ("\u{17B5}", "[U+17B5]"),
            ("\u{180B}", "[U+180B]"),
            ("\u{180E}", "[U+180E]"),
            ("\u{2028}", "[U+2028]"),
            ("\u{2029}", "[U+2029]"),
            ("\u{3164}", "[U+3164]"),
            ("\u{FE00}", "[U+FE00]"),
            ("\u{FE0F}", "[U+FE0F]"),
            ("\u{FFA0}", "[U+FFA0]"),
            ("\u{1D173}", "[U+1D173]"),
            ("\u{1D17A}", "[U+1D17A]"),
            ("\u{E0000}", "[U+E0000]"),
            ("\u{E0001}", "[U+E0001]"),
            ("\u{E0041}", "[U+E0041]"),
            ("\u{E007F}", "[U+E007F]"),
            ("\u{E0100}", "[U+E0100]"),
            ("\u{E01EF}", "[U+E01EF]"),
        ] {
            assert_eq!(escape(input), expected, "{input:?}");
        }
    }

    #[test]
    fn a_tag_block_payload_is_not_carried_through_invisibly() {
        // "ignore" spelled in tag characters is invisible to a reader.
        let hidden: String = "ignore"
            .chars()
            .map(|c| char::from_u32(0xE0000 + c as u32).unwrap())
            .collect();
        let escaped = escape(&format!("hello{hidden}"));
        assert!(escaped.chars().all(|c| (c as u32) < 0xE0000));
        assert!(escaped.starts_with("hello[U+E0069]"));
    }

    #[test]
    fn neighbours_of_the_invisible_ranges_are_left_alone() {
        for visible in [
            "\u{00AC}",
            "\u{00AE}",
            "\u{034E}",
            "\u{1161}",
            "\u{17B6}",
            "\u{3165}",
            "\u{FE10}",
            "\u{FFA1}",
            "\u{1D172}",
            "\u{1D17B}",
            "\u{E01F0}",
            "\u{2030}",
        ] {
            assert_eq!(escape(visible), visible, "{visible:?}");
        }
    }
}
