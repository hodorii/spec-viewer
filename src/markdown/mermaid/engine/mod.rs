//! Pluggable mermaid *graph* diagram renderers (design.md "Pluggable
//! GraphEngine", task 14.1).
//!
//! A [`GraphEngine`] renders one or more graph diagram kinds (flowchart, er,
//! class, state, generic) to terminal lines. An engine may also declare
//! `"sequence"` support (currently only `dg`); when the selected engine does,
//! `render_mermaid` tries it first for sequence diagrams and falls back to
//! the dedicated `seq::layout` compact arrow-list renderer on error or when
//! no selected engine supports `"sequence"`.
//!
//! The registry is a process-global, read-mostly list seeded with the
//! [`builtin`] engine. [`select`]/[`current`] track the single active engine;
//! [`render_mermaid`] applies the fallback rule: when the selected engine
//! does not `supports` a diagram kind, the `builtin` engine renders it
//! instead.

use super::parse::Diagram;
use super::Fallback;
use std::sync::{OnceLock, RwLock};

pub mod builtin;

#[cfg(feature = "engine-dg")]
pub mod dg_engine;
pub mod mdview;

/// A renderer for mermaid graph diagrams. `Send + Sync` so engines can be
/// shared through the process-global registry.
pub trait GraphEngine: Send + Sync {
    /// Stable, user-facing engine name (e.g. "builtin"); also the value of
    /// the CLI's `--diagram-engine <NAME>` argument.
    fn name(&self) -> &'static str;

    /// Whether this engine can render the given diagram kind ("flowchart",
    /// "er", "class", "state", "generic").
    fn supports(&self, kind: &str) -> bool;

    /// Render a parsed graph diagram to terminal lines at `width` columns.
    fn render(&self, src: &str, diagram: &Diagram, width: u16) -> Result<Vec<String>, Fallback>;
}

/// Process-global registry plus selection state.
struct EngineState {
    /// Registered engines, oldest first; "builtin" is always present. The
    /// slice is a box leaked into the process lifetime: the registry only
    /// grows (on [`register`]), so a `'static` snapshot stays valid forever.
    engines: &'static [&'static dyn GraphEngine],
    /// The active engine (defaults to "builtin").
    current: &'static dyn GraphEngine,
}

static STATE: OnceLock<RwLock<EngineState>> = OnceLock::new();

fn state() -> &'static RwLock<EngineState> {
    STATE.get_or_init(|| {
        let mut all: Vec<&'static dyn GraphEngine> =
            vec![&builtin::BuiltinEngine as &'static dyn GraphEngine, &mdview::MdviewEngine as &'static dyn GraphEngine];
        #[cfg(feature = "engine-dg")]
        all.push(&dg_engine::DgEngine as &'static dyn GraphEngine);
        let engines: &'static [&'static dyn GraphEngine] = Box::leak(all.into_boxed_slice());
        RwLock::new(EngineState {
            engines,
            // task 14.2: mdview is the default — its band-routed wiring
            // (subgraph-aware layering, barycenter ordering, dedicated
            // feedback-edge gutters) fixes 5.20's shared-vertical-bus
            // problem that `builtin` still has. `builtin` stays registered
            // and remains the automatic fallback for any kind an engine
            // doesn't `supports()` (currently none — mdview supports every
            // kind builtin does).
            // dg (~/tools/dg) is the default when built with 
            // (on by default): LR/TB, subgraph bands, layer folding to fit
            // narrow widths, crossing-avoiding channels. mdview stays
            // registered and is the default without the feature.
            current: default_engine(),
        })
    })
}

#[cfg(feature = "engine-dg")]
fn default_engine() -> &'static dyn GraphEngine {
    &dg_engine::DgEngine
}

#[cfg(not(feature = "engine-dg"))]
fn default_engine() -> &'static dyn GraphEngine {
    &mdview::MdviewEngine
}

/// All registered engines (the authoritative registry; "builtin" first).
pub fn engines() -> &'static [&'static dyn GraphEngine] {
    state().read().unwrap().engines
}

/// Register an additional engine (future engines, e.g. the mdview engine task
/// 14.2 adds, and tests). Returns `Err` if a same-named engine is already
/// registered.
pub fn register(engine: &'static dyn GraphEngine) -> Result<(), String> {
    let mut st = state().write().unwrap();
    if st.engines.iter().any(|e| e.name() == engine.name()) {
        return Err(format!("diagram engine '{}' is already registered", engine.name()));
    }
    let mut all: Vec<&'static dyn GraphEngine> = st.engines.to_vec();
    all.push(engine);
    st.engines = Box::leak(all.into_boxed_slice());
    Ok(())
}

/// Select the active engine by name. Unknown names return an `Err` whose
/// message lists the available engines (the CLI prints it to stderr and
/// exits 2).
pub fn select(name: &str) -> Result<(), String> {
    let st = state().read().unwrap();
    match st.engines.iter().find(|e| e.name() == name) {
        Some(&engine) => {
            drop(st);
            state().write().unwrap().current = engine;
            Ok(())
        }
        None => {
            let list = st.engines.iter().map(|e| e.name()).collect::<Vec<_>>().join(", ");
            Err(format!("unknown diagram engine '{name}' (available: {list})"))
        }
    }
}

/// The currently selected engine.
pub fn current() -> &'static dyn GraphEngine {
    state().read().unwrap().current
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::mermaid::render_mermaid;

    /// An engine that supports nothing: selecting it must never change render
    /// output, because `render_mermaid` falls back to `builtin`.
    struct DummyEngine;

    impl GraphEngine for DummyEngine {
        fn name(&self) -> &'static str {
            "dummy"
        }
        fn supports(&self, _kind: &str) -> bool {
            false
        }
        fn render(&self, _src: &str, _diagram: &Diagram, _width: u16) -> Result<Vec<String>, Fallback> {
            unreachable!("DummyEngine is never asked to render: supports() is always false")
        }
    }

    #[test]
    fn engines_registry_includes_builtin() {
        assert!(
            engines().iter().any(|e| e.name() == "builtin"),
            "registry must always contain the builtin engine"
        );
    }

    #[test]
    fn select_unknown_engine_errs_with_available_list() {
        let err = select("nope").unwrap_err();
        assert!(
            err.contains("nope"),
            "error should name the rejected engine: {err}"
        );
        assert!(
            err.contains("builtin"),
            "error should list available engines: {err}"
        );
    }

    #[test]
    fn unsupported_selected_engine_falls_back_to_builtin() {
        // Reset to a deterministic baseline (other tests run in parallel and
        // may have left a different selection active).
        select("builtin").unwrap();
        let src = "graph LR\n  A --> B";
        let baseline = render_mermaid(src, 80).unwrap();

        // Register + select an engine that supports nothing...
        register(&DummyEngine).expect("dummy engine is registered only by this test");
        select("dummy").expect("dummy engine should be selectable once registered");
        assert_eq!(current().name(), "dummy");

        // ...and rendering must still yield the builtin output (fallback).
        assert_eq!(render_mermaid(src, 80).unwrap(), baseline);

        // Leave the global selection pristine for later tests / real runs.
        select("builtin").expect("builtin is always selectable");
    }
}