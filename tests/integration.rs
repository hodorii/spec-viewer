//! Integration scenarios for task 8.1 (design.md "Testing Strategy" ->
//! **Integration**): the five end-to-end checks that run against the real
//! `tests/fixtures/kiro` tree and a real filesystem watcher, rather than
//! against injected `DirSnapshot`s.
//!
//! 1. root load        -- `load_snapshot` -> `spec::build` (requirement 3.5)
//! 2. non-UTF-8        -- `load_doc` lossy decode shows U+FFFD (8.4)
//! 3. change within 2s -- `watch::start` delivers an `FsEvent` (7.1)
//! 4. deletion         -- `load_doc` -> `Missing`, reducer -> `Deleted` (7.5)
//! 5. manual mode      -- `watch::start` on a bad root / `watch::manual` (7.6)
//!
//! Each test is independent: the ones that need to mutate files first copy
//! the fixture tree into a unique directory under `std::env::temp_dir()` and
//! remove it at the end, so the checked-in fixtures are never written to
//! (requirement 8.1 applies to the app, but it is good hygiene here too).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use spec_viewer::app::{self, Action, AppState, Control, DocView, WatchStatus};
use spec_viewer::app::loader::{load_doc, load_snapshot};
use spec_viewer::spec::{self, DocKind, NodeId, TreeSource};
use spec_viewer::watch::{self, FsEvent, Watch};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

// --- helpers ---------------------------------------------------------------

fn fixtures_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/kiro"))
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create destination dir");
    for entry in fs::read_dir(src).expect("read source dir") {
        let entry = entry.expect("dir entry");
        let dest_path = dst.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path);
        } else {
            fs::copy(entry.path(), &dest_path).expect("copy fixture file");
        }
    }
}

/// Fresh, unique copy of `tests/fixtures/kiro` under the OS temp dir. The
/// returned path is the `.kiro` root itself (i.e. what `find_root` would
/// return), suitable for `load_snapshot` / `watch::start` directly.
fn temp_fixture_copy(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "spec_viewer_integration_{name}_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    copy_dir_recursive(&fixtures_root(), &dir);
    dir
}

fn enter() -> Action {
    Action::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
}

// --- 1. root load (3.5) -------------------------------------------------------

#[test]
fn root_load_builds_all_fixture_specs_and_flags_broken_json() {
    let root = fixtures_root();
    let snapshot = load_snapshot(&root);
    let model = spec::build(&snapshot);

    // All eight fixture specs are present exactly once.
    let mut names: Vec<&str> = model.specs.iter().map(|s| s.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "broken-json",
            "bugfix-only",
            "no-approvals",
            "no-checkboxes",
            "non-utf8",
            "sample-billing",
            "sample-signup",
            "with-extra",
        ]
    );

    // The model keeps the loader's spec order verbatim (`spec::build` is
    // documented as "caller decides ordering"), so the built order must be
    // exactly the snapshot order, one-to-one.
    let snapshot_order: Vec<&str> = snapshot.specs.iter().map(|s| s.name.as_str()).collect();
    let model_order: Vec<&str> = model.specs.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(model_order, snapshot_order);

    // Steering docs load alongside the specs.
    assert_eq!(model.steering.len(), 3);

    // 3.5: a spec whose spec.json fails to parse is still a node in the
    // model (with its document entries intact), only its meta is `Err`.
    let broken = model
        .specs
        .iter()
        .find(|s| s.name == "broken-json")
        .expect("broken-json spec present");
    assert!(broken.meta.is_err(), "broken-json must have Err meta");
    let req = broken
        .docs
        .iter()
        .find(|d| d.kind == DocKind::Requirements)
        .expect("broken-json still exposes its requirements slot");
    assert!(req.exists, "broken-json/requirements.md exists on disk");
    assert_eq!(req.path, root.join("specs/broken-json/requirements.md"));

    // Sanity: a healthy spec parses fine in the same load.
    let signup = model
        .specs
        .iter()
        .find(|s| s.name == "sample-signup")
        .expect("sample-signup spec present");
    assert!(signup.meta.is_ok());
}

// --- 2. non-UTF-8 (8.4) -------------------------------------------------------

#[test]
fn non_utf8_document_renders_with_replacement_character() {
    let path = fixtures_root().join("specs/non-utf8/requirements.md");

    // Precondition: the fixture really is invalid UTF-8.
    let raw = fs::read(&path).expect("read non-utf8 fixture");
    assert!(
        std::str::from_utf8(&raw).is_err(),
        "fixture must contain invalid UTF-8 bytes"
    );

    let view = load_doc(&path, 80);
    match view {
        DocView::Rendered { path: p, r, .. } => {
            assert_eq!(p, path);
            let plain = r.plain.join("\n");
            // The readable part is kept ...
            assert!(plain.contains("Non-UTF8 bytes should be present"));
            // ... and the broken bytes show up as U+FFFD, not as an error.
            assert!(plain.contains('\u{FFFD}'), "expected U+FFFD in: {plain:?}");
        }
        other => panic!("expected DocView::Rendered, got {other:?}"),
    }
}

// --- 3. change within 2s (7.1) ------------------------------------------------

#[test]
fn file_change_is_reported_within_two_seconds() {
    let root = temp_fixture_copy("change");
    let target = root.join("specs/sample-signup/requirements.md");
    assert!(target.is_file());

    let (tx, rx) = mpsc::channel::<FsEvent>();
    let watch = watch::start(&root, tx);
    assert!(
        matches!(watch, Watch::Live(_)),
        "expected a live watcher on an existing temp dir"
    );

    {
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&target)
            .expect("open requirements.md for append");
        f.write_all(b"\n- 9.9: appended by integration test\n")
            .expect("append");
        f.flush().expect("flush");
    }

    let expected = target.canonicalize().expect("canonicalize target");
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut matched = false;
    // The debouncer may split unrelated events (e.g. directory mtime) from
    // the file write; keep draining until the target path shows up, but
    // never wait past the 2-second budget in total.
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(remaining) {
            Ok(ev) => {
                if ev.paths.iter().any(|p| p == &expected) {
                    matched = true;
                    break;
                }
                seen.extend(ev.paths);
            }
            Err(_) => break,
        }
    }

    drop(watch);
    let _ = fs::remove_dir_all(&root);

    assert!(
        matched,
        "no FsEvent containing {expected:?} within 2s; other paths seen: {seen:?}"
    );
}

// --- 4. deletion (7.5) ----------------------------------------------------------

#[test]
fn deleted_document_resolves_to_missing_from_loader_and_deleted_in_reducer() {
    let root = temp_fixture_copy("delete");
    let path = root.join("specs/sample-signup/design.md");
    assert!(path.is_file());

    // Bring the app to "viewing this document", the same way a user would:
    // build state from disk, select the tree node, press Enter.
    let snapshot = load_snapshot(&root);
    let model = spec::build(&snapshot);
    let mut state = AppState::new(
        TreeSource::Kiro(model),
        root.clone(),
        (120, 40),
        WatchStatus::Live,
        spec_viewer::app::TreeMode::Auto,
        true,
    );
    let selection = vec![
        NodeId::Spec("sample-signup".to_string()),
        NodeId::Doc("sample-signup".to_string(), DocKind::Design),
    ];
    state.tree.select(selection.clone());
    assert_eq!(app::update(&mut state, enter()), Control::Continue);
    match &state.doc {
        DocView::Rendered { path: p, .. } => assert_eq!(p, &path),
        other => panic!("expected Rendered before deletion, got {other:?}"),
    }

    fs::remove_file(&path).expect("delete design.md");

    // Loader contract: a path that no longer exists is `Missing` -- the
    // loader has no memory of whether it used to exist.
    match load_doc(&path, 80) {
        DocView::Missing(p) => assert_eq!(p, path),
        other => panic!("expected DocView::Missing from load_doc, got {other:?}"),
    }

    // Reducer contract (7.5): the *displayed* doc vanishing on an fs event
    // becomes `Deleted` ("파일이 삭제됨"), and the tree selection is kept.
    let fs_event = Action::Fs(FsEvent {
        paths: vec![path.clone()],
    });
    assert_eq!(app::update(&mut state, fs_event), Control::Continue);
    match &state.doc {
        DocView::Deleted(p) => assert_eq!(p, &path),
        other => panic!("expected DocView::Deleted after fs event, got {other:?}"),
    }
    assert_eq!(state.tree.selected().to_vec(), selection);

    let _ = fs::remove_dir_all(&root);
}

// --- 5. manual mode (7.6) ---------------------------------------------------------

#[test]
fn watcher_falls_back_to_manual_mode() {
    // Registration failure -> Manual with a reason.
    let bogus = std::env::temp_dir().join(format!(
        "spec_viewer_integration_does_not_exist_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&bogus);
    assert!(!bogus.exists());

    let (tx, _rx) = mpsc::channel::<FsEvent>();
    match watch::start(&bogus, tx) {
        Watch::Manual(reason) => assert!(!reason.is_empty(), "reason must be non-empty"),
        Watch::Live(_) => panic!("expected Watch::Manual for a nonexistent root"),
    }

    // Explicit opt-out (`--no-watch`) -> Manual carrying the given reason.
    match watch::manual("x") {
        Watch::Manual(reason) => assert_eq!(reason, "x"),
        Watch::Live(_) => panic!("expected Watch::Manual from watch::manual"),
    }
}
