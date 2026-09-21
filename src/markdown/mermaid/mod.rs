#[derive(Debug)]
pub enum Fallback {
    Empty { kind: String },
    Overflow { kind: String },
}

pub mod engine;
mod canvas;
pub mod graph;
pub mod parse;
pub mod seq;

use self::parse::Diagram;

fn sniff_kind(code: &str) -> &'static str {
    let first_line = code.lines().map(|l| l.trim()).find(|l| !l.is_empty());
    match first_line {
        Some(l) if l.starts_with("graph") || l.starts_with("flowchart") => "flowchart",
        Some(l) if l.starts_with("erDiagram") => "er",
        Some(l) if l.starts_with("classDiagram") => "class",
        Some(l) if l.starts_with("stateDiagram") => "state",
        _ => "generic",
    }
}

pub fn render_mermaid(code: &str, width: u16) -> Result<Vec<String>, Fallback> {
    let diagram = parse::parse(code)?;
    let selected = engine::current();
    match diagram {
        Diagram::Graph { .. } => {
            // Fallback rule (design.md "Pluggable GraphEngine"): unsupported
            // kinds render via the builtin engine. `kind` is the *selected
            // engine's own* classification when it has an independent
            // parser to offer one (e.g. `dg`'s eleven mermaid kinds); only
            // when it declines to classify (`None`, the default for
            // `builtin`/`mdview`) do we fall back to this crate's own
            // narrower `sniff_kind`.
            let kind = selected.classify(code).unwrap_or_else(|| sniff_kind(code));
            let renderer = if selected.supports(kind) {
                selected
            } else {
                &engine::builtin::BuiltinEngine
            };
            renderer.render(code, &diagram, width)
        }
        Diagram::Sequence { .. } => {
            // Two-stage fallback: an engine that supports "sequence" (dg's
            // box+lifeline renderer) is tried first; if it errors (e.g. the
            // diagram overflows `width`) or isn't selected/compiled in, we
            // fall back to the dedicated compact `seq::layout` arrow list.
            let kind = selected.classify(code).unwrap_or("sequence");
            if selected.supports(kind) {
                if let Ok(lines) = selected.render(code, &diagram, width) {
                    return Ok(lines);
                }
            }
            seq::layout(&diagram, width)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "engine-dg")]
    use engine::GraphEngine;

    const SEQUENCE_SRC: &str =
        "sequenceDiagram\n    participant A\n    participant B\n    A->>B: hello\n    B-->>A: hi";

    /// task: sequence diagrams route through the dg engine (box+lifeline
    /// rendering) when it's selected and supports "sequence", instead of
    /// always using the compact `seq::layout` arrow list.
    #[cfg(feature = "engine-dg")]
    #[test]
    fn sequence_diagram_renders_via_dg_engine_when_supported() {
        engine::select("dg").expect("dg engine is registered with the engine-dg feature");
        let lines = render_mermaid(SEQUENCE_SRC, 80).expect("fixture must render at a generous width");
        let text = lines.join("\n");
        assert!(
            text.contains('┌') || text.contains('│') || text.contains('▶'),
            "expected dg's box+lifeline rendering, got:\n{text}"
        );
        engine::select("dg").expect("dg engine stays selectable");
    }

    /// task: when dg can't fit the diagram in `width` (returns a `Fallback`),
    /// `render_mermaid` must fall back to the dedicated compact `seq::layout`
    /// arrow-list rendering instead of surfacing the error.
    #[cfg(feature = "engine-dg")]
    #[test]
    fn sequence_diagram_falls_back_to_seq_layout_when_dg_overflows() {
        engine::select("dg").expect("dg engine is registered with the engine-dg feature");
        // At width 12, dg's box+lifeline layout no longer fits (it needs
        // room for two participant boxes plus a lifeline gutter) but the
        // compact `seq::layout` arrow list still does — this is exactly the
        // width band the two-stage fallback exists for.
        let lines = render_mermaid(SEQUENCE_SRC, 12).expect("seq::layout should still find a compact rendering");
        let text = lines.join("\n");
        assert!(!text.contains('┌'), "must not use dg's box rendering at this width:\n{text}");
        assert!(text.contains('►'), "expected seq::layout's compact arrow rendering:\n{text}");
        engine::select("dg").expect("dg engine stays selectable");
    }

    /// `dg` owns its own supported range (`GraphEngine::classify`): a kind
    /// this crate's own `sniff_kind` cannot tell apart from prose (bucketed
    /// as `"generic"`) must still reach `dg`'s real renderer when `dg` is
    /// selected and `dg::diagram::kind_of` recognizes it -- instead of
    /// always landing in `BuiltinEngine`'s crude per-line boxing the way it
    /// did before `classify` existed.
    #[cfg(feature = "engine-dg")]
    #[test]
    fn generic_bucket_kind_reaches_dg_when_dg_recognizes_it() {
        engine::select("dg").expect("dg engine is registered with the engine-dg feature");
        let pie = "pie title status\n  \"Approved\" : 40\n  \"Missing\" : 20\n";
        assert_eq!(sniff_kind(pie), "generic", "precondition: this crate's own sniff_kind can't tell pie apart");
        let lines = render_mermaid(pie, 60).expect("dg renders pie natively");
        let text = lines.join("\n");
        assert!(text.contains('█'), "expected dg's real pie bar chart, got builtin's boxed fallback:\n{text}");
        engine::select("builtin").expect("builtin stays selectable");
    }

    /// A source neither `dg` nor this crate's own `sniff_kind` can classify
    /// at all must still fall back to `BuiltinEngine` (never propagate as an
    /// error, and never be handed to `dg` on the strength of a `"generic"`
    /// label it did not itself assign).
    #[cfg(feature = "engine-dg")]
    #[test]
    fn unclassifiable_source_still_falls_back_to_builtin() {
        engine::select("dg").expect("dg engine is registered with the engine-dg feature");
        let prose = "not a diagram at all\njust some lines\nof plain text\n";
        assert!(
            crate::markdown::mermaid::engine::dg_engine::DgEngine.classify(prose).is_none(),
            "precondition: dg must not recognize plain prose either"
        );
        let lines = render_mermaid(prose, 60).expect("builtin fallback must still succeed");
        assert!(!lines.is_empty());
        engine::select("builtin").expect("builtin stays selectable");
    }
}
