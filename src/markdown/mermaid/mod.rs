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
    match diagram {
        Diagram::Graph { .. } => {
            // Fallback rule (design.md "Pluggable GraphEngine"): unsupported
            // kinds render via the builtin engine.
            let kind = sniff_kind(code);
            let selected = engine::current();
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
            let selected = engine::current();
            if selected.supports("sequence") {
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
}
