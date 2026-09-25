//! End-to-end scenarios, one per `BP-SPEC-VIEW.L2` activity (see
//! `.kiro/specs/spec-viewer/biz-process.md`'s L2 activity table and its
//! `## L1 Process: 스펙 열람` unfold, `L2-A1`..`L2-A6`): "실행 -> 펼침 -> 선택 ->
//! 탐색 -> 변경반영 -> 종료" (this task's own DONE text).
//!
//! Everything each scenario exercises (`spec::find_root`/`build`,
//! `app::AppState::new`/`update`, `app::loader::load_snapshot`, `ui::render`)
//! already has its own unit tests across tasks 1.x-7.2 -- this file's job is
//! only to prove the *whole* reducer-plus-render pipeline holds together for
//! one realistic user story per activity, at the frame level, not just at
//! the reducer-state level.
//!
//! Each test drives its own local `step` (a two-line reducer-plus-render
//! call, mirroring `main.rs`'s own `step()` -- deliberately not imported
//! from there, since `tests/*.rs` integration binaries only link against the
//! `spec_viewer` *library* crate, not the `m` binary crate `main.rs` belongs
//! to).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::Terminal;
use spec_viewer::app::{self, Action, AppState, Control, DocView, Panel, WatchStatus};
use spec_viewer::spec::{self, DocKind, NodeId, TreeSource};
use spec_viewer::watch::FsEvent;
use std::fs;
use std::path::{Path, PathBuf};
use unicode_width::UnicodeWidthStr;

// --- shared helpers ------------------------------------------------------

fn fixtures_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/kiro"))
}

fn build_state_from(root: &Path, size: (u16, u16)) -> AppState {
    let snapshot = app::load_snapshot(root);
    let spec_root = spec::build(&snapshot);
    AppState::new(
        TreeSource::Kiro(spec_root),
        root.to_path_buf(),
        size,
        WatchStatus::Live,
        spec_viewer::app::TreeMode::Auto,
        true,
    )
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Action {
    Action::Mouse(MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE })
}

/// One reducer-plus-render iteration, the same one-liner pattern
/// `main.rs::step` uses (`app::update` then `ui::render`), inlined here
/// since integration tests cannot import from the `m` binary crate.
fn step(terminal: &mut Terminal<TestBackend>, state: &mut AppState, action: Action) -> Control {
    let control = app::update(state, action);
    terminal
        .draw(|f| spec_viewer::ui::render(f, state))
        .expect("draw should succeed");
    control
}

/// Column-width-aware row extraction (same pattern already used by
/// `ui::mod`, `ui::tree_panel`, and `ui::doc_panel`'s own test modules): a
/// wide (e.g. CJK) glyph's hidden continuation cell would otherwise read
/// back as a spurious extra space if columns were joined one at a time.
fn buffer_text(buffer: &Buffer) -> Vec<String> {
    let area = buffer.area();
    (0..area.height)
        .map(|y| {
            let mut row = String::new();
            let mut x = 0u16;
            while x < area.width {
                let symbol = buffer[(x, y)].symbol();
                row.push_str(symbol);
                let w = UnicodeWidthStr::width(symbol).max(1) as u16;
                x += w;
            }
            row
        })
        .collect()
}

fn row_index(rows: &[String], needle: &str) -> usize {
    rows.iter()
        .position(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("expected a row containing {needle:?}, got:\n{rows:?}"))
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create dest dir");
    for entry in fs::read_dir(src).expect("read src dir") {
        let entry = entry.expect("readable dir entry");
        let file_type = entry.file_type().expect("readable file type");
        let dest_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path);
        } else if file_type.is_file() {
            fs::copy(entry.path(), &dest_path).expect("copy fixture file");
        }
    }
}

/// `spec::find_root`'s real directory walk looks for a literal
/// `.kiro`-named child (see its own doc comment/tests in `src/spec/mod.rs`).
/// The checked-in `tests/fixtures/kiro/` is deliberately named `kiro` --
/// *without* the leading dot -- precisely so it is never accidentally
/// picked up by an ancestor search; confirmed by inspection, there is no
/// `.kiro`-named path anywhere under `tests/fixtures/`. So `find_root`
/// cannot see it as-is. To exercise the real `find_root` -> `load_snapshot`
/// -> `build` discovery chain end to end (requirements 1.1, 1.2) without
/// touching `tests/fixtures/`, this copies the checked-in fixtures'
/// (read-only) content into a throwaway scratch dir under an actual
/// `.kiro`-named directory.
fn scratch_root_with_real_kiro(name: &str) -> PathBuf {
    let parent = std::env::temp_dir().join(format!(
        "spec_viewer_app_flow_{name}_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&parent);
    fs::create_dir_all(&parent).expect("create scratch parent dir");
    copy_dir_recursive(&fixtures_root(), &parent.join(".kiro"));
    parent
}

/// Same throwaway-`.kiro`-shaped-directory pattern `app::mod`'s own
/// `resync` tests and `main.rs`'s task 7.2 tests already use (never under
/// `tests/fixtures/`) -- needed here because the L2-A5 scenario edits a file
/// on disk mid-test.
fn temp_kiro_root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "spec_viewer_app_flow_{name}_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("specs")).expect("create specs dir");
    dir
}

fn write_requirements_spec(root: &Path, name: &str, requirements: &str) {
    let dir = root.join("specs").join(name);
    fs::create_dir_all(&dir).expect("create spec dir");
    let spec_json = format!(
        "{{\n  \"name\": \"{name}\",\n  \"created_at\": \"2026-01-01T00:00:00Z\",\n  \"updated_at\": \"2026-01-01T00:00:00Z\",\n  \"language\": \"ko\",\n  \"phase\": \"design\",\n  \"approvals\": {{\n    \"requirements\": {{ \"generated\": true, \"approved\": true }}\n  }}\n}}\n"
    );
    fs::write(dir.join("spec.json"), spec_json).expect("write spec.json");
    fs::write(dir.join("requirements.md"), requirements).expect("write requirements.md");
}

// --- L2-A1: 뷰어 열기 (실행) — requirements 1.1, 1.4 ------------------------

#[test]
fn l2_a1_viewer_opens_with_both_panels_visible_on_first_frame() {
    let parent = scratch_root_with_real_kiro("l2_a1");

    let root = spec::find_root(&parent).expect("find_root should locate the copied .kiro dir");
    assert_eq!(root, parent.join(".kiro"));

    let snapshot = app::load_snapshot(&root);
    let spec_root = spec::build(&snapshot);
    let mut state = AppState::new(
        TreeSource::Kiro(spec_root),
        root,
        (100, 30),
        WatchStatus::Live,
        spec_viewer::app::TreeMode::Auto,
        true,
    );

    // No prior key event at all: `main.rs::run_loop` draws once before ever
    // waiting on input, precisely so the very first frame already shows the
    // "병렬 표시" (requirement 1.4), not only after the user's first
    // keypress.
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|f| spec_viewer::ui::render(f, &mut state))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let rows = buffer_text(&buffer);

    let tree_col = rows
        .iter()
        .find_map(|r| r.find("sample-signup"))
        .expect("expected a real spec name visible in the tree panel on the first frame");
    let doc_col = rows
        .iter()
        .find_map(|r| r.find("문서를 선택하세요"))
        .expect("expected the doc panel's placeholder text visible on the first frame");

    // Side by side, not one replacing the other: tree text left of the
    // ~30% split, doc placeholder at/after it (same layout math `ui::mod`'s
    // own two-panel test uses).
    let split_col = (100f64 * 0.30) as usize;
    assert!(
        tree_col < split_col,
        "expected tree text at column {tree_col} left of the ~30% split at {split_col}"
    );
    assert!(
        doc_col >= split_col,
        "expected doc placeholder at column {doc_col} right of the ~30% split at {split_col}"
    );

    fs::remove_dir_all(&parent).ok();
}

// --- L2-A2: 스펙 현황 파악 (펼침) — requirement 2.2 --------------------------

#[test]
fn l2_a2_expanding_a_spec_shows_docs_in_canonical_order_with_known_status() {
    // sample-signup/spec.json (read directly for this test):
    //   requirements: generated + approved -> ● Approved
    //   bizProcess:   generated, not approved -> ○ Generated
    //   design:       generated, not approved -> ○ Generated
    //   (no "tasks" key at all)               -> ? NoRecord
    // and no research.md / extra files, so the canonical slot order is
    // exactly Requirements -> BizProcess -> Design -> Tasks.
    // Wide enough that the tree panel's 30% share (see `ui::mod`'s
    // `TREE_PANEL_PERCENT`) can fit the full "sample-signup [implementation]"
    // label without truncation -- unlike `ui::tree_panel`'s own unit tests,
    // which give the tree panel the *entire* frame width, this test renders
    // the full `ui::render` composition (tree + doc side by side).
    let mut state = build_state_from(&fixtures_root(), (150, 30));
    state
        .tree
        .open(vec![NodeId::Spec("sample-signup".to_string())]);

    let backend = TestBackend::new(150, 30);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|f| spec_viewer::ui::render(f, &mut state))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let rows = buffer_text(&buffer);

    let spec_row = row_index(&rows, "sample-signup [implementation]");
    let req_row = row_index(&rows, "● requirements.md");
    let biz_row = row_index(&rows, "○ biz-process.md");
    let design_row = row_index(&rows, "○ design.md");
    let tasks_row = row_index(&rows, "? tasks.md");

    assert!(spec_row < req_row, "spec node should come before its docs");
    assert!(
        req_row < biz_row && biz_row < design_row && design_row < tasks_row,
        "expected canonical order requirements -> biz-process -> design -> tasks, got rows: \
         req={req_row} biz={biz_row} design={design_row} tasks={tasks_row}"
    );
}

// --- L2-A3: 문서 읽기 (선택) — requirement 2.6 ------------------------------

#[test]
fn l2_a3_selecting_a_doc_node_and_pressing_enter_renders_its_content() {
    let mut state = build_state_from(&fixtures_root(), (120, 40));
    state.tree.select(vec![
        NodeId::Spec("sample-signup".to_string()),
        NodeId::Doc("sample-signup".to_string(), DocKind::Requirements),
    ]);

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("terminal");

    // `Enter` is bound to the "select" action (keymap.rs) -- a real key
    // event through the real reducer, not a synthetic non-key `Action`.
    let control = step(&mut terminal, &mut state, Action::Key(key(KeyCode::Enter)));
    assert_eq!(control, Control::Continue);

    match &state.doc {
        DocView::Rendered { path, .. } => {
            assert_eq!(path.file_name().unwrap(), "requirements.md");
        }
        other => panic!("expected Rendered, got {other:?}"),
    }

    let buffer = terminal.backend().buffer().clone();
    let rows = buffer_text(&buffer);
    assert!(
        rows.iter()
            .any(|r| r.contains("Validate user input on signup form")),
        "expected the doc panel's next frame to show sample-signup/requirements.md's real content, got:\n{rows:?}"
    );
}

// --- L2-A4: 문서 안 이동·검색 (탐색) — requirement 6.1 -----------------------

#[test]
fn l2_a4_scrolling_within_a_doc_changes_the_visible_top_line_across_frames() {
    // Width 20 wraps sample-signup/requirements.md into 12 `plain` lines
    // (far more than fit in a small viewport), giving real scroll room --
    // same technique `app::mod`'s own `load_wrapped_doc` reducer-test helper
    // uses on this exact fixture file.
    let mut state = build_state_from(&fixtures_root(), (20, 10));
    // Width 20 is below the 80-column narrow-mode threshold, so only the
    // focused panel renders (ui/mod.rs) -- put focus on Doc so its frames
    // are what this test inspects.
    state.focus = Panel::Doc;
    state.tree.select(vec![
        NodeId::Spec("sample-signup".to_string()),
        NodeId::Doc("sample-signup".to_string(), DocKind::Requirements),
    ]);

    let backend = TestBackend::new(20, 10);
    let mut terminal = Terminal::new(backend).expect("terminal");
    step(&mut terminal, &mut state, Action::Key(key(KeyCode::Enter)));

    let total_lines = match &state.doc {
        DocView::Rendered { r, .. } => r.plain.len(),
        other => panic!("expected Rendered, got {other:?}"),
    };
    assert!(
        total_lines > 6,
        "fixture doc too short at this width to exercise scrolling: {total_lines} lines"
    );

    let frame_before = terminal.backend().buffer().clone();
    let rows_before = buffer_text(&frame_before);
    // Row 0 is the doc panel's top border; row 1 is its first content line.
    let top_row_before = rows_before[1].clone();

    // `j` is bound to "line_down" (keymap.rs) -- drive it through several
    // real `step` calls, one key at a time, rather than mutating
    // `state.scroll` directly.
    for _ in 0..4 {
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Char('j'))));
    }
    assert_eq!(state.scroll, 4);

    let frame_after = terminal.backend().buffer().clone();
    let rows_after = buffer_text(&frame_after);
    let top_row_after = rows_after[1].clone();

    assert_ne!(
        top_row_before, top_row_after,
        "expected the doc panel's visible top line to actually change after scrolling, \
         before={top_row_before:?} after={top_row_after:?}"
    );
}

// --- L2-A5: 갱신 지켜보기 (변경반영) — requirement 7.1 -----------------------

#[test]
fn l2_a5_editing_a_loaded_doc_on_disk_and_dispatching_fs_event_updates_the_next_frame() {
    // The one flow in this file that edits a file on disk mid-test -- uses a
    // dedicated throwaway `.kiro` fixture, never the checked-in
    // `tests/fixtures/kiro/` (which a different task, 8.1, is actively
    // using elsewhere).
    let root = temp_kiro_root("l2_a5");
    write_requirements_spec(&root, "watch-demo", "# Before\n\noriginal content line\n");

    let mut state = build_state_from(&root, (120, 40));
    state.focus = Panel::Doc;
    state.tree.select(vec![
        NodeId::Spec("watch-demo".to_string()),
        NodeId::Doc("watch-demo".to_string(), DocKind::Requirements),
    ]);

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("terminal");
    step(&mut terminal, &mut state, Action::Key(key(KeyCode::Enter)));

    let path = match &state.doc {
        DocView::Rendered { path, .. } => path.clone(),
        other => panic!("expected Rendered, got {other:?}"),
    };

    let frame_before = terminal.backend().buffer().clone();
    let rows_before = buffer_text(&frame_before);
    assert!(
        rows_before.iter().any(|r| r.contains("original content line")),
        "sanity: expected the original content visible before the edit"
    );

    fs::write(&path, "# After\n\nupdated content line\n").expect("edit doc on disk");

    // This crate's own convention (task 5.4): any `Fs` event triggers one
    // full resync regardless of which paths changed, so a deliberately
    // empty `paths` list is the right value to construct here -- see
    // `app::update`'s own `Action::Fs` handling in `src/app/mod.rs`.
    step(&mut terminal, &mut state, Action::Fs(FsEvent { paths: vec![] }));

    match &state.doc {
        DocView::Rendered { .. } => {}
        other => panic!("expected still Rendered after resync, got {other:?}"),
    }

    let frame_after = terminal.backend().buffer().clone();
    let rows_after = buffer_text(&frame_after);
    assert!(
        rows_after.iter().any(|r| r.contains("updated content line")),
        "expected the next rendered frame to show the updated file content, got:\n{rows_after:?}"
    );
    assert!(
        !rows_after.iter().any(|r| r.contains("original content line")),
        "did not expect the stale content still visible after resync, got:\n{rows_after:?}"
    );

    fs::remove_dir_all(&root).ok();
}

// --- L2-A6: 뷰어 닫기 (종료) — requirement 1.5 ------------------------------

#[test]
fn l2_a6_quit_key_returns_quit_control_from_a_fully_built_state() {
    // Real terminal restoration (this activity's other requirement, 8.6) is
    // a separate, already-covered, main.rs-only concern (task 7.1):
    // `TestBackend` has no raw-mode/alt-screen concept to assert against, so
    // it is not retestable at this library-only level (see main.rs's own
    // module doc comment). This test's job is only to confirm the reducer
    // signals quit correctly (requirement 1.5) from a full, realistically
    // built `AppState` -- driven directly through `app::update`; no render
    // is needed since `Control::Quit` alone is what is under test here.
    let mut state = build_state_from(&fixtures_root(), (120, 40));
    state
        .tree
        .select(vec![NodeId::Spec("sample-signup".to_string())]);

    // `q` is bound to "quit" (keymap.rs).
    let control = app::update(&mut state, Action::Key(key(KeyCode::Char('q'))));

    assert_eq!(control, Control::Quit);
}

// --- Task 13.1: mouse + file-view E2E (BP-SPEC-VIEW.L2) --------------------
//
// Requirements 1.6, 9.1, 9.2, 9.4, 9.7, 9.9 -- one continuous, realistic
// user story per this task's own DONE text ("클릭 포커스 -> 휠 -> 트리 클릭
// 열기 -> 드래그 선택 -> 복사"), at the frame level, the same "prove the
// whole pipeline holds together" spirit as the L2-A1..A6 scenarios above.

#[test]
fn l2_mouse_click_wheel_tree_open_drag_select_copy() {
    let root = temp_kiro_root("mouse_e2e");
    write_requirements_spec(&root, "alpha", "alpha line one\n\nalpha line two\n\nalpha line three\n");
    write_requirements_spec(&root, "beta", "beta line one\n\nbeta line two\n");

    let mut state = build_state_from(&root, (120, 40));
    let sink = app::clipboard::TestSink::default();
    state.clipboard = Box::new(sink.clone());

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("terminal");

    // First frame: populates `state.layout` (mouse hit-testing) and the
    // tree widget's own flattened row list (`TreeState::rendered_at`,
    // which `key_up`/`key_down`/click hit-testing all need).
    step(&mut terminal, &mut state, Action::Tick);
    assert_eq!(state.focus, Panel::Tree, "sanity: default focus is Tree");

    // Phase 1 (9.1 "클릭 포커스"): a click inside the doc panel -- still
    // empty, nothing selected yet -- moves focus there regardless.
    let doc_area = state.layout.doc;
    step(&mut terminal, &mut state, mouse(MouseEventKind::Down(MouseButton::Left), doc_area.x + 2, doc_area.y + 2));
    assert_eq!(state.focus, Panel::Doc, "click on the doc panel should focus it");

    // Phase 2 (9.2 "휠"): wheel over the TREE panel moves its selection
    // even though Doc is the currently focused panel.
    let tree_area = state.layout.tree;
    assert_eq!(state.tree.selected(), Vec::<NodeId>::new(), "sanity: nothing selected yet");
    step(&mut terminal, &mut state, mouse(MouseEventKind::ScrollDown, tree_area.x + 2, tree_area.y + 2));
    assert_ne!(
        state.tree.selected(),
        Vec::<NodeId>::new(),
        "wheel over the tree should move its selection even while Doc is focused"
    );

    // Phase 3 (9.4/9.5 "트리 클릭 열기"): click "alpha"'s own row to
    // select+expand it (a folder click also toggles), then click its
    // newly-visible requirements.md row to select+load it.
    let rows = buffer_text(terminal.backend().buffer());
    let alpha_row = row_index(&rows, "alpha [design]") as u16;
    step(&mut terminal, &mut state, mouse(MouseEventKind::Down(MouseButton::Left), tree_area.x + 2, alpha_row));
    assert_eq!(state.tree.selected(), [NodeId::Spec("alpha".to_string())]);
    assert!(
        state.tree.opened().contains(&vec![NodeId::Spec("alpha".to_string())]),
        "expected the folder click to expand alpha"
    );
    assert_eq!(state.focus, Panel::Tree, "the tree click should have refocused the tree panel");

    let rows = buffer_text(terminal.backend().buffer());
    let req_row = row_index(&rows, "requirements.md") as u16;
    step(&mut terminal, &mut state, mouse(MouseEventKind::Down(MouseButton::Left), tree_area.x + 4, req_row));
    assert_eq!(
        state.tree.selected(),
        [NodeId::Spec("alpha".to_string()), NodeId::Doc("alpha".to_string(), DocKind::Requirements)]
    );
    let doc_path = match &state.doc {
        DocView::Rendered { path, .. } => path.clone(),
        other => panic!("expected alpha's requirements.md loaded, got {other:?}"),
    };
    assert!(doc_path.ends_with("requirements.md"), "got {doc_path:?}");

    // Phase 4 (9.7 "드래그 선택"): drag-select part of the now-loaded text.
    let doc_area = state.layout.doc; // re-fetch: layout is refreshed every frame
    let inner_x = doc_area.x + 1;
    let inner_y = doc_area.y + 1;
    step(&mut terminal, &mut state, mouse(MouseEventKind::Down(MouseButton::Left), inner_x, inner_y));
    step(&mut terminal, &mut state, mouse(MouseEventKind::Drag(MouseButton::Left), inner_x + 5, inner_y));
    assert_eq!(state.focus, Panel::Doc, "drag-selecting inside the doc panel should have refocused it");

    let expected_copy = {
        let sel = state.selection.expect("expected an active selection mid-drag");
        let r = match &state.doc {
            DocView::Rendered { r, .. } => r,
            other => panic!("expected Rendered, got {other:?}"),
        };
        let (start, end) = sel.ordered();
        assert_eq!(start.0, end.0, "sanity: this drag stayed on one line");
        r.plain[start.0].chars().skip(start.1).take(end.1 - start.1).collect::<String>()
    };
    assert_eq!(expected_copy, "alpha", "sanity: dragged exactly the word 'alpha'");

    // The selection must render reverse-video before it is even released.
    let mid_drag_buffer = terminal.backend().buffer().clone();
    assert!(
        mid_drag_buffer.get(inner_x, inner_y).modifier.contains(ratatui::style::Modifier::REVERSED),
        "expected the dragged range to already show reverse video before Up"
    );

    // Phase 5 (9.9 "복사"): releasing the drag copies exactly that text.
    step(&mut terminal, &mut state, mouse(MouseEventKind::Up(MouseButton::Left), inner_x + 5, inner_y));
    assert_eq!(sink.last(), Some(expected_copy));
    assert_eq!(state.drag, None, "the gesture should have ended");

    fs::remove_dir_all(&root).ok();
}

// --- Task 13.1: `m design.md` file-view flow (requirement 1.6) ------------

#[test]
fn l2_file_view_opens_a_direct_md_path_at_full_width_with_root_found_from_it() {
    // Mirrors what `main.rs::main()` actually does for a `.md` path
    // argument (root found from the file's own location via
    // `spec::find_root`'s ancestor search, then `initial_tree_visible` +
    // `loader::load_doc`), driven here at the library level since
    // integration tests cannot import the `m` binary crate's own `main`.
    let parent = scratch_root_with_real_kiro("file_view");
    let file_path = parent.join(".kiro/specs/sample-signup/requirements.md");
    assert!(file_path.is_file(), "sanity: fixture file exists at {file_path:?}");

    let root = spec::find_root(&file_path).expect("find_root should locate .kiro from the file's own path");
    assert_eq!(root, parent.join(".kiro"), "requirement 1.6: 루트는 파일 위치에서 탐지");

    let snapshot = app::load_snapshot(&root);
    let spec_root = spec::build(&snapshot);
    let size = (120, 40);
    let tree_visible = app::initial_tree_visible(app::TreeMode::Auto, true, size.0);
    assert!(!tree_visible, "requirement 1.7: Auto + file arg -> tree hidden");

    let mut state = AppState::new(TreeSource::Kiro(spec_root), root, size, WatchStatus::Live, app::TreeMode::Auto, tree_visible);
    state.doc = app::load_doc(&file_path, size.0);

    let backend = TestBackend::new(size.0, size.1);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|f| spec_viewer::ui::render(f, &mut state))
        .expect("draw should succeed");

    // No tree content anywhere (excluding the status bar, which always
    // shows the loaded doc's own path and would otherwise false-positive
    // on the spec name it legitimately contains).
    let buffer = terminal.backend().buffer().clone();
    let rows = buffer_text(&buffer);
    let non_status_rows = &rows[..rows.len() - 1];
    assert!(
        !non_status_rows.iter().any(|r| r.contains("sample-signup")),
        "did not expect any tree panel content in file view"
    );

    // The doc panel fills the entire frame width.
    assert_eq!(state.layout.tree, ratatui::layout::Rect::default());
    assert_eq!(state.layout.sep, ratatui::layout::Rect::default());
    assert_eq!(state.layout.doc.width, size.0);

    // And it is really requirements.md's own content, not a placeholder.
    match &state.doc {
        DocView::Rendered { path, .. } => assert_eq!(path, &file_path),
        other => panic!("expected the file argument's own content loaded, got {other:?}"),
    }

    fs::remove_dir_all(&parent).ok();
}

// --- Task 17.2: `--all` mode E2E (requirements 1.8, 1.9, 7.4) --------------
//
// Deliberately *not* named `.kiro` anywhere in its path (unlike
// `temp_kiro_root`/`scratch_root_with_real_kiro` above) -- the whole point
// of this fixture is a plain markdown tree with no `.kiro` schema at all.

fn temp_files_root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "spec_viewer_app_flow_all_{name}_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("sub")).expect("create sub dir");
    dir
}

#[test]
fn l2_all_mode_browses_a_plain_markdown_tree_with_fold_select_and_live_edits() {
    let root = temp_files_root("main_flow");
    fs::write(root.join("top.md"), "# Top\n\ntop-level content\n").expect("write top.md");
    fs::write(
        root.join("sub/note.md"),
        "# Note\n\noriginal content line\n",
    )
    .expect("write note.md");
    let sub_dir = root.join("sub");
    let note_path = sub_dir.join("note.md");

    // No `.kiro` anywhere -- requirement 1.8's "`.kiro` 없이도 동작". Built
    // exactly as `main.rs::resolve_source`'s `--all` branch does: `FsTree`
    // straight off the real directory, no `find_root` call at all.
    let tree = spec::FsTree::scan(&root);
    let mut state = AppState::new(
        TreeSource::Files(tree),
        root.clone(),
        (100, 30),
        WatchStatus::Live,
        spec_viewer::app::TreeMode::Auto,
        true,
    );

    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|f| spec_viewer::ui::render(f, &mut state))
        .expect("draw");

    // First frame: both top-level entries visible, "Files" title (not
    // "Specs"), and the folded "sub" folder's own child not yet visible.
    let rows = buffer_text(&terminal.backend().buffer().clone());
    assert!(rows.iter().any(|r| r.contains("Files")), "expected the Files-mode tree title, got:\n{rows:?}");
    assert!(rows.iter().any(|r| r.contains("top.md")));
    assert!(rows.iter().any(|r| r.contains("sub")));
    assert!(
        !rows.iter().any(|r| r.contains("note.md")),
        "folder must start folded, got:\n{rows:?}"
    );
    // Requirement 1.9: no spec `[phase]` badge anywhere -- `render_files`
    // never builds the badge/status-symbol/`n/m` spans `render_kiro` does.
    // (Not asserting against "●"/"○" here: `Tree`'s own scrollbar thumb
    // legitimately renders "●" on the block border regardless of source.)
    assert!(!rows.iter().any(|r| r.contains('[')));

    // Fold -> unfold: select the folder, press Right to open it.
    state.tree.select(vec![NodeId::Dir(sub_dir.clone())]);
    step(&mut terminal, &mut state, Action::Key(key(KeyCode::Right)));
    let rows = buffer_text(&terminal.backend().buffer().clone());
    assert!(
        rows.iter().any(|r| r.contains("note.md")),
        "expected note.md visible once 'sub' is unfolded, got:\n{rows:?}"
    );

    // Unfold -> fold again: press Left to close it back up.
    step(&mut terminal, &mut state, Action::Key(key(KeyCode::Left)));
    let rows = buffer_text(&terminal.backend().buffer().clone());
    assert!(
        !rows.iter().any(|r| r.contains("note.md")),
        "expected note.md hidden again once 'sub' is refolded, got:\n{rows:?}"
    );

    // A folder has no "정의" concept (requirement 1.9) -- selecting it loads
    // nothing into the doc panel.
    step(&mut terminal, &mut state, Action::Key(key(KeyCode::Enter)));
    match &state.doc {
        DocView::Empty => {}
        other => panic!("a Dir node must not resolve to any doc/definition, got {other:?}"),
    }

    // Select the file itself (re-open the folder first, matching a real
    // user's fold -> select path) and confirm its real content renders.
    step(&mut terminal, &mut state, Action::Key(key(KeyCode::Right)));
    state.tree.select(vec![NodeId::Dir(sub_dir.clone()), NodeId::File(note_path.clone())]);
    step(&mut terminal, &mut state, Action::Key(key(KeyCode::Enter)));
    match &state.doc {
        DocView::Rendered { path, .. } => assert_eq!(path, &note_path),
        other => panic!("expected note.md's content loaded, got {other:?}"),
    }
    let rows = buffer_text(&terminal.backend().buffer().clone());
    assert!(
        rows.iter().any(|r| r.contains("original content line")),
        "expected note.md's real content in the doc panel, got:\n{rows:?}"
    );

    // Requirement 1.9's watch parity: an on-disk edit under the `--all`
    // scan root reaches the doc panel the same way a `.kiro` edit does
    // (mirrors `l2_a5` above, for `TreeSource::Files` instead of `Kiro`).
    fs::write(&note_path, "# Note\n\nupdated content line\n").expect("edit note.md on disk");
    step(&mut terminal, &mut state, Action::Fs(FsEvent { paths: vec![] }));
    let rows = buffer_text(&terminal.backend().buffer().clone());
    assert!(
        rows.iter().any(|r| r.contains("updated content line")),
        "expected the edited content reflected after resync, got:\n{rows:?}"
    );
    assert!(!rows.iter().any(|r| r.contains("original content line")));

    // And a new file dropped under the scan root shows up in the tree too
    // (requirement 7.4's directory-rescan-on-add path).
    fs::write(root.join("fresh.md"), "# Fresh\n").expect("add a new file");
    step(&mut terminal, &mut state, Action::Fs(FsEvent { paths: vec![] }));
    let rows = buffer_text(&terminal.backend().buffer().clone());
    assert!(
        rows.iter().any(|r| r.contains("fresh.md")),
        "expected a newly added file to appear in the tree after resync, got:\n{rows:?}"
    );

    fs::remove_dir_all(&root).ok();
}
