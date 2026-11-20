//! Mode round trips driven only by real key presses: every switch goes
//! through the reducer's key dispatch, the scan/watch wiring and a real
//! render, then is judged on the rendered frame, not only on reducer state.

use super::tests::{args_with_path, build_state_from, fixtures_root, key, scratch_dir};
use super::*;
use crossterm::event::KeyCode;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;
use ratatui::Terminal;
use spec_viewer::app::{AppState, Control};
use spec_viewer::spec::{DocKind, NodeId, TreeSource};
use std::fs;
use std::path::{Path, PathBuf};
use unicode_width::UnicodeWidthStr;

pub(crate) const FRAME_SIZE: (u16, u16) = (150, 60);

/// A running viewer: the same state, terminal, watch and sync a real run
/// owns, fed one key at a time like `run_loop` does.
pub(crate) struct Session {
    pub(crate) state: AppState,
    pub(crate) terminal: Terminal<TestBackend>,
    project: PathBuf,
    args: Args,
    tx: std::sync::mpsc::Sender<spec_viewer::watch::FsEvent>,
    _rx: std::sync::mpsc::Receiver<spec_viewer::watch::FsEvent>,
    watch: spec_viewer::watch::Watch,
    sync: WatchDirSync,
}

impl Session {
    /// Opens spec mode over `<project>/.kiro` as if launched from `project`.
    pub(crate) fn open_spec_mode(project: &Path) -> Self {
        let mut state = build_state_from(&project.join(".kiro"), FRAME_SIZE);
        state.launch_dir = project.to_path_buf();
        // Startup sorts the specs by the active key; without it a fresh
        // session would list them in raw scan order, unlike a switched-back one.
        let sort_key = state.sort_key;
        if let TreeSource::Kiro(spec_root) = &mut state.root {
            spec_viewer::spec::sort_specs(&mut spec_root.specs, sort_key);
        }
        let mut args = args_with_path(Some(project.to_path_buf()));
        // Watching is not what is under test, and a manual watch keeps many
        // sessions from exhausting inotify instances.
        args.no_watch = true;
        let (tx, rx) = std::sync::mpsc::channel();
        let mut terminal = Terminal::new(TestBackend::new(FRAME_SIZE.0, FRAME_SIZE.1)).expect("terminal");
        terminal.draw(|f| spec_viewer::ui::render(f, &mut state)).expect("first draw");
        Session {
            state,
            terminal,
            project: project.to_path_buf(),
            args,
            tx,
            _rx: rx,
            watch: spec_viewer::watch::manual("no-watch"),
            sync: WatchDirSync::default(),
        }
    }

    pub(crate) fn press(&mut self, code: KeyCode) -> Control {
        let control = step(&mut self.terminal, &mut self.state, spec_viewer::app::Action::Key(key(code)))
            .expect("step");
        if control == Control::SwitchMode {
            return switch_source_and_sync_watch(
                &mut self.terminal,
                &mut self.state,
                &self.project,
                &self.args,
                &self.tx,
                &mut self.watch,
                &mut self.sync,
            )
            .expect("switch");
        }
        control
    }

    pub(crate) fn press_space(&mut self) {
        self.press(KeyCode::Char(' '));
    }

    pub(crate) fn expand_everything(&mut self) {
        self.press(KeyCode::Char('o'));
    }

    pub(crate) fn is_files_mode(&self) -> bool {
        matches!(self.state.root, TreeSource::Files(_))
    }

    /// Moves the cursor with real Down/Up presses until it rests on `target`.
    pub(crate) fn move_cursor_to(&mut self, target: &[NodeId]) {
        for code in [KeyCode::Down, KeyCode::Up] {
            for _ in 0..400 {
                if self.state.tree.selected() == target {
                    return;
                }
                self.press(code);
            }
        }
        assert_eq!(self.state.tree.selected(), target, "cursor never reached the target");
    }

    /// Path of a file inside the files tree currently shown.
    pub(crate) fn files_node_path(&self, file: &Path) -> Vec<NodeId> {
        match &self.state.root {
            TreeSource::Files(tree) => tree
                .node_path_for(file)
                .unwrap_or_else(|| panic!("{} is not in the files tree", file.display())),
            _ => panic!("not in files mode"),
        }
    }

    /// The tree panel only: everything left of its own top-right corner,
    /// without the full-width status bar on the last row.
    pub(crate) fn tree_rows(&self) -> Vec<String> {
        let buffer = self.terminal.backend().buffer();
        let width = tree_panel_width(buffer);
        let panel_row_count = usize::from(buffer.area().height) - 1;
        frame_rows(buffer)
            .into_iter()
            .take(panel_row_count)
            .map(|row| take_columns(&row, width))
            .collect()
    }

    /// Text of the row the tree draws as the cursor.
    pub(crate) fn highlighted_tree_row(&self) -> String {
        let buffer = self.terminal.backend().buffer();
        let width = tree_panel_width(buffer);
        let rows = frame_rows(buffer);
        let highlighted: Vec<String> = (1..buffer.area().height)
            .filter(|&y| buffer[(1, y)].modifier.contains(Modifier::REVERSED))
            .map(|y| take_columns(&rows[usize::from(y)], width))
            .collect();
        assert_eq!(highlighted.len(), 1, "expected exactly one cursor row, got {highlighted:?}");
        highlighted.into_iter().next().unwrap()
    }
}

fn frame_rows(buffer: &Buffer) -> Vec<String> {
    let area = buffer.area();
    (0..area.height)
        .map(|y| {
            let mut row = String::new();
            let mut x = 0u16;
            while x < area.width {
                let symbol = buffer[(x, y)].symbol();
                row.push_str(symbol);
                x += UnicodeWidthStr::width(symbol).max(1) as u16;
            }
            row
        })
        .collect()
}

fn tree_panel_width(buffer: &Buffer) -> u16 {
    (0..buffer.area().width)
        .find(|&x| buffer[(x, 0)].symbol() == "┐")
        .map(|x| x + 1)
        .expect("tree panel top-right corner on the first row")
}

fn take_columns(row: &str, columns: u16) -> String {
    let mut taken = String::new();
    let mut used = 0usize;
    for ch in row.chars() {
        let width = UnicodeWidthStr::width(ch.to_string().as_str());
        if used + width > usize::from(columns) {
            break;
        }
        used += width;
        taken.push(ch);
    }
    taken
}

fn copy_dir_recursive(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create dir");
    for entry in fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("dir entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir_recursive(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

/// A project whose `.kiro` is a copy of the checked-in fixtures, plus a
/// nested folder in one spec so folds matter in files mode.
fn project_with_fixture_kiro(name: &str) -> PathBuf {
    let project = scratch_dir(name);
    copy_dir_recursive(&fixtures_root(), &project.join(".kiro"));
    let notes = project.join(".kiro/specs/sample-signup/notes");
    fs::create_dir_all(&notes).expect("create notes dir");
    fs::write(notes.join("deep.md"), "# Deep\n").expect("write deep.md");
    project
}

fn signup_doc(kind: DocKind) -> Vec<NodeId> {
    vec![NodeId::Spec("sample-signup".to_string()), NodeId::Doc("sample-signup".to_string(), kind)]
}

fn signup_dir(project: &Path) -> PathBuf {
    project.join(".kiro/specs/sample-signup")
}

fn files_root(session: &Session) -> PathBuf {
    match &session.state.root {
        TreeSource::Files(tree) => tree.root.clone(),
        _ => panic!("not in files mode"),
    }
}

fn assert_cursor_row_names(session: &Session, file_name: &str) {
    let row = session.highlighted_tree_row();
    assert!(row.contains(file_name), "expected the cursor row to be {file_name}, got {row:?}");
}

#[test]
fn space_round_trip_returns_to_the_spec_cursor_with_its_ancestors_expanded_on_screen() {
    let project = project_with_fixture_kiro("rt_spec_files_spec");
    let mut session = Session::open_spec_mode(&project);
    session.expand_everything();
    session.move_cursor_to(&signup_doc(DocKind::Design));
    assert!(session.tree_rows()[0].contains("Specs ["));

    session.press_space();

    assert!(session.is_files_mode());
    assert_eq!(files_root(&session), project, "the tree starts above the cursor's spec");
    assert!(session.tree_rows()[0].contains("Files ["), "tree title names the files tree");
    let tree_text = session.tree_rows().join("\n");
    assert!(tree_text.contains("requirements.md"), "the cursor spec's own files are listed");
    assert!(
        tree_text.contains("sample-billing"),
        "a spec listed before the cursor stays visible, not cut off"
    );
    assert_cursor_row_names(&session, "design.md");

    session.press_space();

    assert!(!session.is_files_mode());
    assert!(session.tree_rows()[0].contains("Specs ["));
    assert_eq!(session.state.tree.selected(), signup_doc(DocKind::Design).as_slice());
    assert_cursor_row_names(&session, "design.md");
    assert!(
        session.tree_rows().join("\n").contains("sample-billing"),
        "the whole spec tree is back, not just the cursor's spec"
    );
    fs::remove_dir_all(&project).ok();
}

#[test]
fn files_round_trip_over_the_same_root_restores_a_cursor_inside_a_nested_folder() {
    let project = project_with_fixture_kiro("rt_files_spec_files");
    let mut session = Session::open_spec_mode(&project);
    session.expand_everything();
    session.move_cursor_to(&signup_doc(DocKind::Design));
    session.press_space();
    session.expand_everything();
    let deep = session.files_node_path(&signup_dir(&project).join("notes/deep.md"));
    session.move_cursor_to(&deep);
    assert_cursor_row_names(&session, "deep.md");

    session.press_space();
    assert!(!session.is_files_mode());
    session.press_space();

    assert!(session.is_files_mode());
    assert_eq!(files_root(&session), project, "the spec cursor maps to the same root");
    assert_eq!(session.state.tree.selected(), deep.as_slice());
    assert_cursor_row_names(&session, "deep.md");
    assert!(
        session.tree_rows().join("\n").contains("notes"),
        "the restored cursor is reachable because its folder is open"
    );
    fs::remove_dir_all(&project).ok();
}

#[test]
fn a_moved_cursor_is_what_the_next_round_trip_restores_in_both_modes() {
    let project = project_with_fixture_kiro("rt_updated_restore");
    let mut session = Session::open_spec_mode(&project);
    session.expand_everything();
    session.move_cursor_to(&signup_doc(DocKind::Design));
    session.press_space();
    session.expand_everything();
    let tasks = session.files_node_path(&signup_dir(&project).join("tasks.md"));
    session.move_cursor_to(&tasks);
    session.press_space();
    assert_eq!(session.state.tree.selected(), signup_doc(DocKind::Design).as_slice());

    session.move_cursor_to(&signup_doc(DocKind::Requirements));
    session.press_space();
    assert_eq!(
        session.state.tree.selected(),
        tasks.as_slice(),
        "same root: the remembered files cursor wins over the spec cursor's own file"
    );
    let biz_process = session.files_node_path(&signup_dir(&project).join("biz-process.md"));
    session.move_cursor_to(&biz_process);

    session.press_space();
    assert_eq!(
        session.state.tree.selected(),
        signup_doc(DocKind::Requirements).as_slice(),
        "the spec cursor moved since the first round trip"
    );
    assert_cursor_row_names(&session, "requirements.md");
    session.press_space();
    assert_eq!(session.state.tree.selected(), biz_process.as_slice(), "the files cursor moved too");
    assert_cursor_row_names(&session, "biz-process.md");
    fs::remove_dir_all(&project).ok();
}

#[test]
fn a_cursor_in_another_spec_enters_files_mode_on_that_specs_own_file() {
    let project = project_with_fixture_kiro("rt_other_root");
    let mut session = Session::open_spec_mode(&project);
    session.expand_everything();

    let billing_requirements = vec![
        NodeId::Spec("sample-billing".to_string()),
        NodeId::Doc("sample-billing".to_string(), DocKind::Requirements),
    ];
    session.move_cursor_to(&billing_requirements);
    session.press_space();

    assert_eq!(files_root(&session), project);
    assert_cursor_row_names(&session, "requirements.md");
    assert_eq!(
        session.state.tree.selected().last(),
        Some(&NodeId::File(project.join(".kiro/specs/sample-billing/requirements.md"))),
        "the cursor lands on the other spec's own file"
    );
    fs::remove_dir_all(&project).ok();
}

#[test]
fn a_position_saved_by_an_earlier_run_does_not_override_the_cursor_file_in_a_new_session() {
    let project = project_with_fixture_kiro("rt_stale_saved_position");
    let mut previous_run = Session::open_spec_mode(&project);
    previous_run.expand_everything();
    previous_run.move_cursor_to(&signup_doc(DocKind::Design));
    previous_run.press_space();
    let requirements = previous_run.files_node_path(&signup_dir(&project).join("requirements.md"));
    previous_run.move_cursor_to(&requirements);
    spec_viewer::app::remember_tree_position(&mut previous_run.state);
    let saved = previous_run.state.remembered_tree_positions.clone();

    let mut new_session = Session::open_spec_mode(&project);
    new_session.state.remembered_tree_positions = saved;
    new_session.expand_everything();
    new_session.move_cursor_to(&signup_doc(DocKind::Design));
    new_session.press_space();

    assert!(new_session.is_files_mode());
    assert_cursor_row_names(&new_session, "design.md");

    new_session.move_cursor_to(&requirements);
    new_session.press_space();
    new_session.press_space();
    assert_eq!(
        new_session.state.tree.selected(),
        requirements.as_slice(),
        "within the same session a shown tree is restored"
    );
    fs::remove_dir_all(&project).ok();
}
