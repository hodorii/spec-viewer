//! The `dg` [`GraphEngine`](super::GraphEngine) — delegates graph diagrams to
//! the `dg` crate (`~/tools/dg`, MIT), a layered layout with TB/LR, subgraph
//! bands, layer folding to fit narrow widths, crossing-avoiding channel
//! ordering and `◠` hops at edge crossings. Compiled only with the
//! `engine-dg` feature; select with `--diagram-engine dg`.

use super::super::parse::Diagram;
use super::super::Fallback;
use super::GraphEngine;

pub struct DgEngine;

impl GraphEngine for DgEngine {
    fn name(&self) -> &'static str {
        "dg"
    }

    fn supports(&self, kind: &str) -> bool {
        matches!(kind, "flowchart" | "er" | "class" | "state" | "sequence")
    }

    fn render(&self, src: &str, _diagram: &Diagram, width: u16) -> Result<Vec<String>, Fallback> {
        let kind = ::dg::diagram::kind_of(::dg::Language::Mermaid, src).unwrap_or("flowchart").to_string();
        // Plain text only: color is applied later by code.rs, as with the other engines.
        let options = ::dg::RenderOptions { width: usize::from(width), diagram_caption: false, ..::dg::RenderOptions::default() };
        match ::dg::render_diagram(src, Some(::dg::Language::Mermaid), &options) {
            Some(lines) => Ok(lines.iter().map(|line| line.text().to_string()).collect()),
            // dg gives up only when nothing fits the width (it already retried LR/TB and folding).
            None => Err(Fallback::Overflow { kind }),
        }
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
}
