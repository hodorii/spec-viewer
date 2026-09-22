//! The `dg` [`GraphEngine`](super::GraphEngine) — delegates graph diagrams to
//! the `dg` crate (`~/tools/dg`, MIT), a layered layout with TB/LR, subgraph
//! bands, layer folding to fit narrow widths, crossing-avoiding channel
//! ordering and `◠` hops at edge crossings. Compiled only with the
//! `engine-dg` feature; select with `--diagram-engine dg`.
//!
//! `dg` owns its own supported range: [`classify`](GraphEngine::classify)
//! asks `dg::diagram::kind_of` directly instead of consulting a whitelist
//! copied here, so every kind `dg`'s own mermaid parser recognizes
//! (flowchart/sequence/state/er/class plus gitGraph/block-beta/pie/
//! xychart-beta/quadrantChart/gantt, none of which this crate's own
//! `sniff_kind` tells apart from plain prose) gets a real chance at `dg`'s
//! renderer instead of silently landing in this crate's `"generic"` bucket.
//!
//! The same delegation covers PlantUML via
//! [`render_other_language`](GraphEngine::render_other_language): this
//! crate has no PlantUML parser or IR of its own at all (unlike mermaid,
//! where `builtin`/`mdview` at least have this crate's own `Diagram`), so
//! whether a `plantuml`/`puml`/`uml` fence becomes a real diagram is
//! entirely up to whichever engine is selected -- currently only `dg`
//! answers at all (`dg::diagram::language_of_fence` decides which fence
//! languages count).
//!
//! [`render_document`](GraphEngine::render_document) goes one step further
//! and hands `dg` the *whole markdown document* (headings, prose, lists,
//! tables, quotes, code -- diagram fences included, rendered inline by
//! `dg` itself): `dg::render_markdown` returns `dg::Line`s, whose
//! `(text, Style)` runs convert 1:1 to this crate's `Span`/
//! `SpanStyle::Raw` (both `dg::Style` and `ratatui::style::Style` are a
//! plain fg/bg-color-plus-modifier-flags struct, so no lossy six-way
//! `SpanStyle` mapping is needed here the way `ui::doc_panel`'s own
//! hardcoded styling requires for this crate's own renderer). Heading/
//! footnote metadata, which `dg` doesn't expose, is reconstructed from the
//! same source via `markdown::delegated`.

use super::super::parse::Diagram;
use super::super::Fallback;
use super::GraphEngine;
use crate::markdown::delegated::{footnotes_in_definition_order, locate_heading_lines, scan_headings_and_footnotes};
use crate::markdown::{Line, Rendered, Span, SpanStyle};

/// `dg::Style`'s fields map 1:1 onto `ratatui::style::Style`'s (indexed
/// fg/bg color plus independent bold/dim/italic/underline/strike/reverse
/// flags) -- this is a direct field-by-field conversion, not a heuristic.
fn dg_style_to_ratatui(style: ::dg::Style) -> ratatui::style::Style {
    use ratatui::style::{Color, Modifier, Style as RStyle};

    let mut out = RStyle::default();
    if let ::dg::style::Color::Indexed(i) = style.fg {
        out = out.fg(Color::Indexed(i));
    }
    if let ::dg::style::Color::Indexed(i) = style.bg {
        out = out.bg(Color::Indexed(i));
    }
    let mut modifier = Modifier::empty();
    if style.bold {
        modifier |= Modifier::BOLD;
    }
    if style.dim {
        modifier |= Modifier::DIM;
    }
    if style.italic {
        modifier |= Modifier::ITALIC;
    }
    if style.underline {
        modifier |= Modifier::UNDERLINED;
    }
    if style.strike {
        modifier |= Modifier::CROSSED_OUT;
    }
    if style.reverse {
        modifier |= Modifier::REVERSED;
    }
    out.add_modifier(modifier)
}

pub struct DgEngine;

/// Call `dg::render_diagram` for `language` and turn its `Option<Vec<Line>>`
/// into this crate's `Result<Vec<String>, Fallback>` — shared by mermaid
/// (`render`) and every other diagram language `dg` knows
/// (`render_other_language`).
fn render_via_dg(language: ::dg::Language, src: &str, width: u16) -> Result<Vec<String>, Fallback> {
    let kind = ::dg::diagram::kind_of(language, src).unwrap_or("flowchart").to_string();
    // Plain text only: color is applied later by code.rs, as with the other engines.
    let options = ::dg::RenderOptions { width: usize::from(width), diagram_caption: false, ..::dg::RenderOptions::default() };
    match ::dg::render_diagram(src, Some(language), &options) {
        Some(lines) => Ok(lines.iter().map(|line| line.text().to_string()).collect()),
        // dg gives up only when nothing fits the width (it already retried LR/TB and folding).
        None => Err(Fallback::Overflow { kind }),
    }
}

impl GraphEngine for DgEngine {
    fn name(&self) -> &'static str {
        "dg"
    }

    fn supports(&self, kind: &str) -> bool {
        // `render_mermaid` only ever asks with a `kind` that either came
        // from our own `classify` below (in which case `dg` already
        // recognized it -- anything but `"generic"`) or, when `classify`
        // returned `None`, from this crate's own `sniff_kind` fallback
        // (whose vocabulary is a subset of `dg`'s, so reaching that branch
        // at all means `dg` didn't recognize the source either, and
        // `sniff_kind` can then only have produced `"generic"`). Either
        // way, "not generic" is exactly "dg's own classify said yes".
        kind != "generic"
    }

    fn classify(&self, src: &str) -> Option<&'static str> {
        ::dg::diagram::kind_of(::dg::Language::Mermaid, src)
    }

    fn render(&self, src: &str, _diagram: &Diagram, width: u16) -> Result<Vec<String>, Fallback> {
        render_via_dg(::dg::Language::Mermaid, src, width)
    }

    fn render_other_language(&self, lang: &str, src: &str, width: u16) -> Option<Result<Vec<String>, Fallback>> {
        // `dg`'s own fence-language mapping decides eligibility, not a
        // hand-copied list here -- mirrors how `classify` defers to
        // `dg::diagram::kind_of` for mermaid's own diagram-kind detection.
        let language = ::dg::diagram::language_of_fence(lang)?;
        if language == ::dg::Language::Mermaid {
            // The `"mermaid"`/`"mmd"` fence tags are already handled by
            // `code.rs`'s dedicated mermaid path (`render_mermaid`), which
            // also covers `sniff_kind`'s own vocabulary for `builtin`/
            // `mdview` -- this hook only needs to add languages this crate
            // has no IR of its own for at all.
            return None;
        }
        Some(render_via_dg(language, src, width))
    }

    fn render_document(&self, src: &str, width: u16) -> Option<Rendered> {
        let options = ::dg::RenderOptions {
            width: usize::from(width),
            block_width: None,
            theme: ::dg::Theme::dark(),
            diagram: ::dg::DiagramOptions::default(),
            // No "◈ mermaid · flowchart ───" captions inline in a document
            // view -- matches how the mermaid/PlantUML fence paths above
            // already suppress them (code.rs draws its own fence framing).
            diagram_caption: false,
        };
        let dg_lines = ::dg::render_markdown(src, &options);

        let mut lines = Vec::with_capacity(dg_lines.len());
        let mut plain = Vec::with_capacity(dg_lines.len());
        for dg_line in &dg_lines {
            let spans: Vec<Span> = dg_line
                .runs()
                .map(|(text, style)| Span { text: text.to_string(), style: SpanStyle::Raw(dg_style_to_ratatui(style)) })
                .collect();
            plain.push(dg_line.text().to_string());
            lines.push(Line { indent: 0, spans, style: None });
        }

        let (heading_defs, footnote_defs) = scan_headings_and_footnotes(src);
        let headings = locate_heading_lines(&plain, &heading_defs);
        let footnotes = footnotes_in_definition_order(footnote_defs);

        Some(Rendered { lines, plain, headings, footnotes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::mermaid::parse;

    #[test]
    fn renders_flowchart_with_arrow() {
        let src = "graph LR\n  A --> B";
        let diagram = parse::parse(src).unwrap();
        let lines = DgEngine.render(src, &diagram, 80).unwrap();
        let joined = lines.join("\n");
        assert!(joined.contains("▶") || joined.contains("▼"), "{joined}");
        assert!(!joined.contains("◈"), "caption must not leak into engine output: {joined}");
    }

    #[test]
    fn render_other_language_renders_plantuml_sequence() {
        let src = "@startuml\nAlice -> Bob: hi\n@enduml\n";
        let lines = DgEngine
            .render_other_language("plantuml", src, 80)
            .expect("dg must recognize a plantuml fence")
            .expect("plantuml sequence should fit at width 80");
        let joined = lines.join("\n");
        assert!(joined.contains("Alice") && joined.contains("Bob"), "{joined}");
    }

    #[test]
    fn render_other_language_recognizes_puml_and_uml_aliases_too() {
        let src = "@startuml\nAlice -> Bob: hi\n@enduml\n";
        assert!(DgEngine.render_other_language("puml", src, 80).is_some());
        assert!(DgEngine.render_other_language("uml", src, 80).is_some());
    }

    #[test]
    fn render_other_language_declines_mermaid_and_unknown_languages() {
        // "mermaid" is code.rs's own dedicated path; render_other_language
        // must not double-handle it.
        assert!(DgEngine.render_other_language("mermaid", "graph LR\n  A --> B", 80).is_none());
        assert!(DgEngine.render_other_language("rust", "fn main() {}", 80).is_none());
    }
}
