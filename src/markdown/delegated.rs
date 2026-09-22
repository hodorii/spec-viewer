//! Helpers for a [`GraphEngine::render_document`](super::mermaid::engine::GraphEngine::render_document)
//! implementation that delegates the *whole* markdown document to an
//! external library (currently only `dg`): that library gives back styled
//! lines and nothing else, so this crate's own `HeadingAnchor`/`Footnote`
//! bookkeeping — needed by the TOC popup, `[`/`]` heading navigation, and
//! resize scroll-position preservation — has to be reconstructed separately
//! from the same source, then matched back onto the delegated renderer's
//! plain-text line output.
//!
//! Kept independent of any specific delegated engine (no `dg` import here)
//! so a future engine wrapping a different library can reuse it the same
//! way — matches this crate's "an engine's own capability, not a whitelist
//! here" principle (see `mermaid::engine` module docs).

use super::{Footnote, HeadingAnchor};

/// (level, heading text), in document order.
type HeadingDefs = Vec<(u8, String)>;
/// (footnote name, body text), in definition order.
type FootnoteDefs = Vec<(String, String)>;

/// Scan `src` for heading text (in document order, with level) and footnote
/// definitions (in definition order, with body text) via a lightweight
/// pulldown-cmark pass — cheap metadata extraction, not a rendering pass, so
/// it stays accurate regardless of how the visual lines were wrapped/styled.
pub(crate) fn scan_headings_and_footnotes(src: &str) -> (HeadingDefs, FootnoteDefs) {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

    let parser = Parser::new_ext(src, Options::ENABLE_FOOTNOTES);
    let mut headings: Vec<(u8, String)> = Vec::new();
    let mut footnotes: Vec<(String, String)> = Vec::new();
    let mut in_heading: Option<u8> = None;
    let mut heading_text = String::new();
    let mut in_footnote: Option<String> = None;
    let mut footnote_text = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                in_heading = Some(level as u8);
                heading_text.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(level) = in_heading.take() {
                    headings.push((level, heading_text.trim().to_string()));
                }
            }
            Event::Start(Tag::FootnoteDefinition(name)) => {
                in_footnote = Some(name.to_string());
                footnote_text.clear();
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                if let Some(name) = in_footnote.take() {
                    footnotes.push((name, footnote_text.trim().to_string()));
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if in_heading.is_some() {
                    heading_text.push_str(&t);
                }
                if in_footnote.is_some() {
                    footnote_text.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if in_heading.is_some() {
                    heading_text.push(' ');
                }
                if in_footnote.is_some() {
                    footnote_text.push(' ');
                }
            }
            _ => {}
        }
    }

    (headings, footnotes)
}

/// Find each heading's line index within `plain` (the delegated renderer's
/// own plain-text lines), matching whole-line text in order and advancing a
/// cursor so repeated heading text resolves to distinct occurrences. A
/// heading that can't be located exactly (e.g. wrapped across lines by the
/// delegated renderer) is silently skipped rather than guessed at — a
/// missing TOC entry degrades gracefully; a wrong one would jump the reader
/// somewhere unrelated.
pub(crate) fn locate_heading_lines(plain: &[String], headings: &[(u8, String)]) -> Vec<HeadingAnchor> {
    let mut result = Vec::with_capacity(headings.len());
    let mut cursor = 0usize;
    for (level, text) in headings {
        let trimmed = text.trim();
        if let Some(offset) = plain[cursor..].iter().position(|line| line.trim() == trimmed) {
            let line = cursor + offset;
            result.push(HeadingAnchor { level: *level, text: trimmed.to_string(), line });
            cursor = line + 1;
        }
    }
    result
}

/// Turn footnote definitions (in the order `scan_headings_and_footnotes`
/// encountered them) into this crate's own [`Footnote`] list, numbered
/// 1-based in that order. Note: this crate's own renderer numbers footnotes
/// by first *reference* order (`FootnoteState::ref_index`); a delegated
/// renderer's footnote *display* order is whatever it chose, and nothing in
/// the app actually reads `Rendered.footnotes` today (test-only field), so
/// definition order here is a deliberate simplification, not a bug.
pub(crate) fn footnotes_in_definition_order(defs: Vec<(String, String)>) -> Vec<Footnote> {
    defs.into_iter()
        .enumerate()
        .map(|(i, (name, text))| Footnote { index: i + 1, name, text })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_headings_in_order_with_level() {
        let (headings, _) = scan_headings_and_footnotes("# Title\nbody\n\n## Sub\nmore");
        assert_eq!(headings, vec![(1, "Title".to_string()), (2, "Sub".to_string())]);
    }

    #[test]
    fn scans_footnote_definitions() {
        let (_, footnotes) = scan_headings_and_footnotes(
            "ref[^a] and[^b]\n\n[^a]: first note\n[^b]: second note",
        );
        assert_eq!(
            footnotes,
            vec![("a".to_string(), "first note".to_string()), ("b".to_string(), "second note".to_string())]
        );
    }

    #[test]
    fn locates_headings_and_advances_past_duplicates() {
        let plain = vec!["Intro".to_string(), "Title".to_string(), "body".to_string(), "Title".to_string()];
        let headings = vec![(1u8, "Title".to_string()), (1u8, "Title".to_string())];
        let anchors = locate_heading_lines(&plain, &headings);
        assert_eq!(anchors.len(), 2);
        assert_eq!(anchors[0].line, 1);
        assert_eq!(anchors[1].line, 3);
    }

    #[test]
    fn missing_heading_is_skipped_not_guessed() {
        let plain = vec!["unrelated line".to_string()];
        let headings = vec![(1u8, "Nowhere".to_string())];
        assert!(locate_heading_lines(&plain, &headings).is_empty());
    }

    #[test]
    fn footnotes_numbered_in_definition_order() {
        let footnotes = footnotes_in_definition_order(vec![
            ("a".to_string(), "first".to_string()),
            ("b".to_string(), "second".to_string()),
        ]);
        assert_eq!(footnotes[0].index, 1);
        assert_eq!(footnotes[0].text, "first");
        assert_eq!(footnotes[1].index, 2);
        assert_eq!(footnotes[1].text, "second");
    }
}
