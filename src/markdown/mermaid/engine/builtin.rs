//! The default [`GraphEngine`]: wraps the existing `graph::layout` renderer
//! behind the engine trait (design.md "Pluggable GraphEngine" -> builtin).
//!
//! Supports every graph diagram kind; it is both the registry default and
//! the fallback target when the selected engine cannot render a kind.

use super::GraphEngine;
use super::super::parse::Diagram;
use super::super::{sniff_kind, Fallback};

/// The built-in renderer -- the pre-pluggability `graph::layout` behind the
/// [`GraphEngine`] trait.
pub struct BuiltinEngine;

impl GraphEngine for BuiltinEngine {
    fn name(&self) -> &'static str {
        "builtin"
    }

    fn supports(&self, kind: &str) -> bool {
        matches!(kind, "flowchart" | "er" | "class" | "state" | "generic")
    }

    fn render(&self, src: &str, diagram: &Diagram, width: u16) -> Result<Vec<String>, Fallback> {
        super::super::graph::layout(diagram, width, sniff_kind(src))
    }
}