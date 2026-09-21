//! Source-text adapter for the optional `graphs-tui` engine (crate 0.4,
//! AGPL-3.0-or-later; cargo feature `engine-graphs-tui`, default off).
//!
//! Unlike `builtin`/`mdview`, this engine does **not** consume the parsed
//! [`Diagram`]: it hands the raw mermaid fence text to
//! `graphs_tui::render_mermaid_to_tui` and returns the rendered lines.
//! Kinds graphs-tui cannot parse (er/class/sequence/) report
//! [`Fallback::Empty`] so callers fall back to the builtin engine
//! (design.md "Pluggable GraphEngine").

use super::GraphEngine;
use super::super::parse::Diagram;
use super::super::Fallback;

/// graphs-tui 0.4 (AGPL-3.0-or-later) behind the [`GraphEngine`] trait.
pub struct GraphsTuiEngine;

impl GraphEngine for GraphsTuiEngine {
    fn name(&self) -> &'static str {
        "graphs-tui"
    }

    fn supports(&self, kind: &str) -> bool {
        matches!(kind, "flowchart" | "graph" | "state" | "stateDiagram")
    }

    fn render(&self, src: &str, _diagram: &Diagram, width: u16) -> Result<Vec<String>, Fallback> {
        let kind = super::super::sniff_kind(src).to_string();

        // graphs-tui parses one statement per line. Mermaid's `;` statement
        // separator allows several statements on one line (`graph LR; A-->B`),
        // so split them into lines before handing the source text over.
        let normalized = src.replace(';', "\n");

        let options = graphs_tui::RenderOptions {
            ascii: false,
            max_width: Some(width as usize),
            ..Default::default()
        };

        let result = graphs_tui::render_mermaid_to_tui(&normalized, options)
            .map_err(|_| Fallback::Empty { kind: kind.clone() })?;

        let lines: Vec<String> = result.output.lines().map(String::from).collect();
        if lines.is_empty() {
            return Err(Fallback::Empty { kind });
        }
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use crate::markdown::mermaid::parse::{Diagram, Dir};
    use super::*;

    fn diagram() -> Diagram {
        // The adapter ignores the parsed diagram entirely (source-text
        // adapter) — a minimal graph is all the trait signature needs.
        Diagram::Graph { dir: Dir::LR, nodes: vec![], edges: vec![], groups: vec![] }
    }

    #[test]
    fn renders_raw_src_with_nodes() {
        let engine = GraphsTuiEngine;
        let lines = engine.render("graph LR; A-->B", &diagram(), 80).unwrap();
        assert!(
            lines.len() >= 1,
            "expected at least one rendered line, got {}",
            lines.len()
        );
        let joined: String = lines.concat();
        assert!(joined.contains('A'), "node A missing in: {joined:?}");
        assert!(joined.contains('B'), "node B missing in: {joined:?}");
    }

    #[test]
    fn er_kind_is_not_supported() {
        let engine = GraphsTuiEngine;
        assert!(!engine.supports("er"), "graphs-tui must not claim er support");
        assert!(!engine.supports("class"), "graphs-tui must not claim class support");
        assert!(engine.supports("flowchart"), "flowchart must be supported");
        assert!(engine.supports("state"), "state must be supported");
    }
}