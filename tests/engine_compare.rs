//! Engine comparison snapshots (requirement 5.22 / design.md "Engine
//! comparison", task 14.4).
//!
//! Extracts every ```mermaid fence from the sample spec dir(s) under
//! `tests/fixtures/mermaid-samples/<spec>/design.md` (currently this crate's
//! own real design doc, copied in), renders each fence at width 100
//! with every registered engine in turn
//! (`spec_viewer::markdown::mermaid::engine::engines()` — engines compiled
//! out by cargo features drop out of the iteration naturally), and writes
//! `tests/snapshots/engines/<spec>-<n>.<engine>.txt` (`<n>` is the 1-based
//! mermaid-fence ordinal inside the file). Existing snapshot files are
//! overwritten. These files are review artifacts only — there is no
//! auto-judgment on their content.
//!
//! Snapshot *writing* happens only when `UPDATE_ENGINE_SNAPSHOTS=1`; the
//! assertions below run on every invocation (never `#[ignore]`d):
//!   1. each engine produces at least one non-empty render across the
//!      extracted fences;
//!   2. 5.21 fallback (design.md "Pluggable GraphEngine"): even for a kind
//!      the engine does not support — `erDiagram` here — the *final*
//!      `render_mermaid` result is `Ok` and non-empty, because the
//!      unsupported kind falls back to the builtin engine.
//!
//! A fence that an engine cannot lay out (`Fallback::Overflow`) is captured
//! exactly the way the application displays it (`markdown::code::
//! push_code_block`): a `Mermaid (Overflow): <kind>` marker followed by the
//! source lines — so the review snapshot always shows the *effective* final
//! output a user of that engine would see at width 100.

use std::fs;
use std::path::{Path, PathBuf};

use spec_viewer::markdown::mermaid::engine::{self, GraphEngine};
use spec_viewer::markdown::mermaid::{render_mermaid, Fallback};

/// The width every fence is rendered at.
const WIDTH: u16 = 100;

/// A diagram of a kind not every engine supports (`supports("er")` can be
/// false), used to drive assertion 2: even with such an engine selected, the
/// 5.21 fallback must still yield an `Ok`, non-empty render via the builtin
/// engine, while `builtin`/`mdview` render it directly.
const ER_DIAGRAM: &str = "erDiagram\n  CUSTOMER ||--o{ ORDER : places";

/// One extracted ```mermaid fence, named by spec dir + 1-based fence ordinal.
struct Fence {
    /// The spec directory name, e.g. "spec-viewer".
    spec: String,
    /// 1-based ordinal of the mermaid fence within that spec's design.md.
    n: usize,
    /// The fence body, without the ``` delimiters.
    src: String,
}

/// Scan `.kiro/specs/*/design.md` under `specs_dir` for ```mermaid fences.
/// Spec dirs are walked in sorted order; fences are numbered per file in
/// document order. Non-mermaid fences are skipped without affecting the
/// mermaid ordinal.
fn extract_mermaid_fences(specs_dir: &Path) -> Vec<Fence> {
    let mut fences = Vec::new();

    let mut entries: Vec<fs::DirEntry> = fs::read_dir(specs_dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", specs_dir.display()))
        .collect::<Result<_, _>>()
        .unwrap_or_else(|e| panic!("read_dir entries {}: {e}", specs_dir.display()));
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let design = entry.path().join("design.md");
        if !design.is_file() {
            continue;
        }
        let text = fs::read_to_string(&design)
            .unwrap_or_else(|e| panic!("read {}: {e}", design.display()));
        let spec = entry.file_name().to_string_lossy().into_owned();

        let mut in_fence = false;
        let mut buf: Vec<&str> = Vec::new();
        let mut n = 0usize;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                if in_fence {
                    fences.push(Fence { spec: spec.clone(), n, src: buf.join("\n") });
                    in_fence = false;
                } else if trimmed.trim_start_matches("```").trim() == "mermaid" {
                    in_fence = true;
                    n += 1;
                    buf.clear();
                }
            } else if in_fence {
                buf.push(line);
            }
        }
        // Tolerate an unclosed trailing fence instead of silently dropping it.
        if in_fence {
            fences.push(Fence { spec, n, src: buf.join("\n") });
        }
    }
    fences
}

/// The final lines the application would display for this fence at `width` —
/// `render_mermaid`'s output (selected engine) when it succeeds, and the
/// `push_code_block` overflow marker + source lines when it does not
/// (`markdown::code::push_code_block`). Never empty.
fn final_output(fence: &Fence, width: u16) -> Vec<String> {
    match render_mermaid(&fence.src, width) {
        Ok(lines) => lines,
        Err(fallback) => {
            let label = match fallback {
                Fallback::Empty { kind } => format!("Mermaid (Empty): {kind}"),
                Fallback::Overflow { kind } => format!("Mermaid (Overflow): {kind}"),
            };
            let mut out = vec![label];
            for line in fence.src.lines() {
                // push_code_block truncates over-long lines to width-1 + '…'.
                // Fence sources in this repo are ASCII, so char-counting is
                // a safe width proxy for the review artifacts.
                if line.chars().count() > width as usize {
                    let truncated: String = line.chars().take(width as usize - 1).collect();
                    out.push(format!("{truncated}…"));
                } else {
                    out.push(line.to_string());
                }
            }
            out
        }
    }
}

/// `tests/snapshots/engines/` under the crate manifest dir.
fn snapshot_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("snapshots").join("engines")
}

fn snapshot_path(fence: &Fence, engine: &dyn GraphEngine) -> PathBuf {
    snapshot_dir().join(format!("{}-{}.{}.txt", fence.spec, fence.n, engine.name()))
}

#[test]
fn engine_comparison_snapshots_and_fallback() {
    let specs_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mermaid-samples");
    assert!(specs_dir.is_dir(), "no specs dir at {}", specs_dir.display());

    let fences = extract_mermaid_fences(&specs_dir);
    assert!(
        !fences.is_empty(),
        "no ```mermaid fences found under {}",
        specs_dir.display()
    );

    let engines = engine::engines();
    assert!(
        engines.len() >= 2,
        "expected at least builtin + mdview in the registry, got {}",
        engines.len()
    );

    let update = std::env::var("UPDATE_ENGINE_SNAPSHOTS").as_deref() == Ok("1");

    // Assertion 1: per-engine count of raw (pre-fallback) non-empty renders
    // across the extracted fences.
    let mut raw_non_empty = vec![0usize; engines.len()];

    for (ei, e) in engines.iter().enumerate() {
        engine::select(e.name())
            .unwrap_or_else(|err| panic!("select engine '{}': {err}", e.name()));
        let name = e.name();

        for fence in &fences {
            // Raw engine path: the Ok render, or None when the engine/model
            // reports fallback — this is what assertion 1 counts.
            if let Ok(lines) = render_mermaid(&fence.src, WIDTH) {
                if !lines.is_empty() {
                    raw_non_empty[ei] += 1;
                }
            }

            if update {
                let path = snapshot_path(fence, *e);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).unwrap_or_else(|err| {
                        panic!("create_dir_all {}: {err}", parent.display())
                    });
                }
                let body = final_output(fence, WIDTH).join("\n") + "\n";
                fs::write(&path, body)
                    .unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
            }
        }

        // Assertion 2 (5.21 fallback): a kind the engine does not support
        // (e.g. "er") must still render Ok and non-empty — the unsupported
        // kind falls back to the builtin engine *inside* render_mermaid.
        let fallback = render_mermaid(ER_DIAGRAM, WIDTH).unwrap_or_else(|f| {
            panic!("engine '{name}' erDiagram render failed: {f:?}")
        });
        assert!(
            !fallback.is_empty(),
            "engine '{name}' produced empty output for unsupported kind erDiagram \
             (5.21 builtin fallback must kick in)",
        );
    }

    for (ei, e) in engines.iter().enumerate() {
        assert!(
            raw_non_empty[ei] >= 1,
            "engine '{}' produced no non-empty raw render across {} fence(s)",
            e.name(),
            fences.len()
        );
    }

    // Leave the process-global selection on the crate default for any later
    // tests in this binary.
    engine::select("mdview").expect("mdview is the crate-default engine and always selectable");
}