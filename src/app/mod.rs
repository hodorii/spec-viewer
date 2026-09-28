pub mod clipboard;
pub mod keymap;
pub mod loader;
pub mod mouse;
pub mod search;

pub use clipboard::Clipboard;

pub use loader::{
    format_modified, format_size, load_definition, load_doc, load_for_selection, load_snapshot,
    DirSnapshot, FileSnapshot, SpecDirSnapshot,
};

use crate::markdown::Rendered;
use crate::spec::{FsEntry, NodeId, SortKey, TreeSource};
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tui_tree_widget::{TreeItem, TreeState};

/// `ui::tree_panel`'s cache for `--all` mode's `Vec<TreeItem>` build
/// (spec-viewer-files-tree-scroll-latency): rebuilding the whole nested
/// `TreeItem` tree -- fresh `String`/`Line`/`Span`/`TreeItem` allocations
/// per entry -- on every single render call made scrolling a large `--all`
/// target (tens of thousands of files) visibly stutter, since a scroll
/// only moves the viewport and never needs the whole tree rebuilt. This
/// holds the last `(entries, search_matches)` a `Vec<TreeItem>` was built
/// from, so `ui::tree_panel::render_files` can reuse it unless either
/// input actually changed; lives on `AppState` the same way `tree:
/// TreeState<NodeId>` already does (both are `tui_tree_widget` widget
/// state, not business data -- `ui` owns writing to it, `app`'s reducer
/// never reads or mutates it).
pub type FilesTreeItemCache = (Vec<FsEntry>, Vec<Vec<NodeId>>, Vec<TreeItem<'static, NodeId>>);

/// The three mouse-hit-testable regions of one frame (design.md "Mouse
/// hit-test from stored layout"): the tree panel, the 1-column drag handle
/// between panels, and the doc panel. Refreshed by `ui::render` every frame
/// so mouse handling (`app::mouse`) always hit-tests against exactly what
/// was actually drawn, not a recomputed-and-possibly-stale guess.
///
/// A hidden/not-rendered panel gets a zero-size `Rect` (`Rect::default()`
/// via `Rect::new(0, 0, 0, 0)`), which `Rect::contains` never matches --
/// no `Option` wrapper needed per field.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PanelLayout {
    pub tree: Rect,
    pub sep: Rect,
    pub doc: Rect,
}

/// Requirement 8.5 / design.md "폭 < 80 → 활성 패널만 전체 폭": below this
/// width, `ui::render` draws only the focused panel, at the full main-area
/// width. Lives in `app` (not `ui`, which depends on `app` but never the
/// reverse -- see `lib.rs`) so `doc_panel_width` below can share the exact
/// threshold `ui::render` uses instead of a second, driftable copy.
pub const NARROW_WIDTH_THRESHOLD: u16 = 80;

/// Two-panel split ratio `ui::render` defaults `state.split` to on its
/// first use. See `NARROW_WIDTH_THRESHOLD` for why this lives here.
pub const TREE_PANEL_PERCENT: u16 = 30;

/// The doc panel's outer width (border columns included) that `ui::render`
/// will actually give it for `state` at `state.size.0` -- the width every
/// per-document loader (`loader::load_doc`/`load_definition`/
/// `load_for_selection`) must be handed so a two-panel frame wraps text to
/// fit the doc panel it is actually drawn into, rather than the full
/// terminal width (tasks.md 20's NO-GO: a paragraph wrapped past the doc
/// panel's right border, silently dropping whole clipped lines instead of
/// visually truncating them).
///
/// Pure and side-effect-free. `ui::render` computes the same tree/doc split
/// independently (it additionally needs the `Rect`s' x/y placement and
/// performs `state.split`'s first-use default assignment, which needs
/// `&mut AppState`) -- by the identical formula, so the two can never drift
/// apart.
pub fn doc_panel_width(state: &AppState) -> u16 {
    let main_width = state.size.0;
    if !state.tree_visible || state.tree_mode == TreeMode::Single || main_width < NARROW_WIDTH_THRESHOLD {
        return main_width;
    }
    let default_split = ((main_width as u32 * TREE_PANEL_PERCENT as u32) / 100) as u16;
    let split = if state.split == 0 { default_split } else { state.split };
    let tree_width = split.min(main_width.saturating_sub(1));
    main_width.saturating_sub(tree_width).saturating_sub(1)
}

/// Single-document view state shown in the doc panel.
///
/// Each non-`Empty` variant is produced by [`loader::load_doc`],
/// [`loader::load_definition`], or [`loader::load_for_selection`] and
/// isolates a per-document failure without crashing the rest of the app
/// (design.md "Error Handling"; requirements 2.4, 2.6, 3.6, 3.7, 7.5, 8.3).
///
/// `Deleted` is not produced by task 5.1's loaders — it is a *reducer*
/// transition (task 5.4) applied to already-displayed content when a live
/// file-delete event arrives. It is included here because design.md's
/// `app::DocView` type declares it for that later task to use.
pub enum DocView {
    Empty,
    Rendered { path: PathBuf, r: Rendered, meta: FileInfo },
    Definition { spec: String, text: Rendered },
    Missing(PathBuf),
    Deleted(PathBuf),
    ReadError { path: PathBuf, msg: String },
    MetaError { spec: String, msg: String },
}

/// File metadata bundled with a rendered doc (requirement 6.12): the modified
/// timestamp, byte size, and source line count. Collected by
/// [`loader::load_doc`] on every load/reload — so a file-change event that
/// reloads the doc refreshes all three values (requirement 7.1) — and
/// displayed by the status bar (design.md line 182's `FileInfo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileInfo {
    pub modified: SystemTime,
    pub size: u64,
    pub lines: usize,
}

// `SystemTime` has no std `Default`, so it is written by hand instead of
// derived: the epoch timestamp is the "no file info" placeholder test
// fixtures use when the meta value itself is irrelevant.
impl Default for FileInfo {
    fn default() -> Self {
        Self {
            modified: SystemTime::UNIX_EPOCH,
            size: 0,
            lines: 0,
        }
    }
}

// `Rendered`/`Line` do not implement `PartialEq` (and deriving `Debug` would
// require them to implement it too), so `DocView` gets a small manual
// `Debug` impl instead of `#[derive(Debug)]`. Tests should still prefer
// pattern-matching/field access over `assert_eq!` on whole `DocView` values.
impl std::fmt::Debug for DocView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocView::Empty => write!(f, "DocView::Empty"),
            DocView::Rendered { path, .. } => write!(f, "DocView::Rendered({path:?})"),
            DocView::Definition { spec, .. } => write!(f, "DocView::Definition({spec:?})"),
            DocView::Missing(p) => write!(f, "DocView::Missing({p:?})"),
            DocView::Deleted(p) => write!(f, "DocView::Deleted({p:?})"),
            DocView::ReadError { path, msg } => {
                write!(f, "DocView::ReadError({path:?}, {msg:?})")
            }
            DocView::MetaError { spec, msg } => write!(f, "DocView::MetaError({spec:?}, {msg:?})"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeMode {
    Always,
    Auto,
    Hidden,
    /// Requirement 1.10 "단일 모드": the tree panel or the doc panel fills
    /// the whole main area (never a side-by-side split, regardless of
    /// width) -- which one is currently showing is tracked by
    /// `AppState::doc_focus`, not by `focus`/width the way `Auto`'s narrow
    /// fallback (8.5) is. Selecting a document switches to the doc panel;
    /// `Esc` switches back to the tree (see `load_selected_doc` and
    /// `handle_key`'s Esc branch).
    Single,
}

/// Determines the initial tree visibility based on mode and startup context.
pub fn initial_tree_visible(mode: TreeMode, is_file_arg: bool, _width: u16) -> bool {
    match mode {
        TreeMode::Always => true,
        TreeMode::Hidden => false,
        TreeMode::Auto => !is_file_arg,
        // Single always starts on the tree side (`doc_focus` defaults to
        // `false` in `AppState::new`) -- a file argument's 1.6 special case
        // does not apply to this mode.
        TreeMode::Single => true,
    }
}

/// Which panel currently receives key input (requirement 2.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Tree,
    Doc,
}

/// Live/manual state of the filesystem watcher (design.md `watch::Watch`
/// surfaced into `AppState` for the status bar; requirement 7.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchStatus {
    Live,
    Manual { reason: String },
}

/// In-document search state (requirements 6.5-6.9). This task only wires it
/// into `AppState` with a sane empty default — the search behavior itself
/// (task 5.3) is out of scope here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchState {
    pub query: String,
    pub matches: Vec<usize>,
    pub current: Option<usize>,
}

/// A modal overlay drawn over the two panels (design.md `ui::popup`).
///
/// `Toc(selected)` carries the index of the currently-highlighted heading
/// within the current doc's `Rendered.headings` (task 5.6, requirements 6.3,
/// 6.4). `SearchInput(buffer)` carries the in-progress, not-yet-confirmed
/// query being typed (task 5.6, requirement 6.5) -- kept separate from
/// `AppState::search.query` so that canceling (`Esc`) never clobbers a prior
/// confirmed search. `Message` remains unused by any requirement here; it
/// gets a minimal Esc-to-dismiss for consistency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Popup {
    Toc(usize),
    /// The bindings list plus the currently-highlighted row (mirrors
    /// `Toc(usize)`'s own "index alongside the data" shape): `render_help`
    /// (ui::popup) uses a stateful `List` keyed off this index so the view
    /// auto-scrolls to keep it visible -- needed because the entry count
    /// (34 as of `spec-viewer-tree-navigation-modes`) exceeds what fits in
    /// the popup's fixed-height area on a realistically sized terminal,
    /// confirmed via a real pty smoke test during that spec's verification
    /// (a defect this session found and fixed: without scrolling, several
    /// bindings near the end of `BINDINGS` -- including `m`/`o`/`c` and
    /// `e` -- were unreachable in the help screen on any terminal shorter
    /// than roughly 60 rows).
    Help(Vec<(String, String)>, usize),
    SearchInput(String),
    Message(String),
}

/// The single application state struct threaded through `update` (design.md
/// "app — State & Reducer").
pub struct AppState {
    /// What the tree panel is browsing (requirement 1.8): the existing
    /// `.kiro`-schema `Kiro(SpecRoot)`, or a plain markdown directory tree
    /// `Files(FsTree)` for `--all` mode.
    pub root: TreeSource,
    /// The directory `root` was (re)built from -- `.kiro` normally, or the
    /// `--all` scan directory -- needed to re-read from disk on
    /// `Action::Fs`/`Refresh` (requirements 7.3, 7.4, 7.5, 7.6; see
    /// `resync`).
    pub kiro_root: PathBuf,
    pub tree_mode: TreeMode,
    pub tree_visible: bool,
    /// `TreeMode::Single` only (requirement 1.10): `false` shows the tree
    /// panel at full width, `true` shows the doc panel at full width.
    /// Flipped to `true` by `load_selected_doc` when a selection actually
    /// loads something, and back to `false` by `Esc` (`handle_key`).
    /// Meaningless (left at whatever it was) under every other `TreeMode`.
    pub doc_focus: bool,
    pub tree: TreeState<NodeId>,
    /// Requirement 2.9: the spec tree's current sort key. Defaults to
    /// `SortKey::Name` in `AppState::new` -- a caller that wants a
    /// different `--sort` starting value assigns this field directly
    /// afterwards (same post-construction-override pattern `clipboard`
    /// uses below), and must also sort `root`'s specs to match before the
    /// first render, since this field alone does not reorder anything (see
    /// `cycle_sort`/`resync`, the only two places that keep the two in
    /// sync at runtime).
    pub sort_key: SortKey,
    pub focus: Panel,
    pub doc: DocView,
    pub scroll: usize,
    pub search: SearchState,
    /// Requirement 2.10: the tree panel's own search state, kept separate
    /// from `search` (the doc panel's) so focusing one panel's search never
    /// clobbers the other's. Reuses `SearchState` (and, in `ui::status_bar`,
    /// the same rendering path) so both searches share one visual/keying
    /// convention -- `matches` here is only ever `(0..tree_matches.len())`,
    /// a placeholder the status bar's "(i/n)" count reads; the actual
    /// matched node paths live in `tree_matches`.
    pub tree_search: SearchState,
    /// The tree nodes `tree_search.query` currently matches, in flattened
    /// tree order, kept in lockstep with `tree_search.matches`/`current`
    /// (same length, same indices) by every `app::search::tree_*` function.
    pub tree_matches: Vec<Vec<NodeId>>,
    pub popup: Option<Popup>,
    pub watch: WatchStatus,
    pub size: (u16, u16),
    /// This frame's panel rectangles (design.md "Mouse hit-test from stored
    /// layout"), refreshed by `ui::render` every draw. Starts zeroed --
    /// meaningless until the first frame renders.
    pub layout: PanelLayout,
    /// Tree panel width in columns for two-panel mode (requirement 9.6).
    /// `0` means "not yet sized" -- `ui::render` picks a default fraction
    /// of the frame width the first time it lays out two panels, then
    /// leaves this value alone (task 12.4 will let the user drag it).
    pub split: u16,
    /// The in-progress mouse gesture, if any (requirements 9.6, 9.7).
    /// `None` between a button `Up` and the next `Down`.
    pub drag: Option<Drag>,
    /// The current doc-panel text selection, if any (requirement 9.7).
    /// Outlives the drag itself -- `Up` ends `drag` but leaves `selection`
    /// visible/copyable (task 12.6) until it is explicitly cleared.
    pub selection: Option<Selection>,
    /// Set while a `Drag::Select` gesture's pointer sits at or beyond the
    /// doc panel's top/bottom edge (requirement 9.7's "포인터가 패널 상/하
    /// 경계에 닿으면"); consumed one row at a time on `Action::Tick`.
    pub auto_scroll: Option<AutoScrollDir>,
    /// Where a copied selection's text goes (requirement 9.9). Real runs
    /// get `clipboard::Osc52`; tests swap in `clipboard::TestSink` by
    /// assigning this field directly after `AppState::new` (design.md:
    /// "Clipboard 트레이트 뒤에 두어 테스트는 싱크로 대체") -- no need to
    /// thread it through `new`'s own parameter list.
    pub clipboard: Box<dyn Clipboard>,
    /// `ui::tree_panel`'s `--all`-mode `Vec<TreeItem>` build cache (see
    /// [`FilesTreeItemCache`]). `None` until the first `Files`-mode render.
    pub files_tree_cache: Option<FilesTreeItemCache>,
}

/// A doc-panel text selection, in document `(line, col)` coordinates
/// (design.md `Selection { anchor, head }`) -- `anchor` is where the drag
/// started, `head` is where the pointer currently is. Order is not fixed:
/// `head` can be before or after `anchor` depending on drag direction; see
/// [`Selection::ordered`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: (usize, usize),
    pub head: (usize, usize),
}

impl Selection {
    /// `(start, end)` with `start <= end` in (line, col) reading order,
    /// regardless of which of `anchor`/`head` the drag actually started
    /// from -- the only order `doc_panel`'s highlight math needs.
    pub fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoScrollDir {
    Up,
    Down,
}

/// Which gesture a left-button drag is currently performing (design.md
/// "app — State & Reducer").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drag {
    Split,
    Select,
}

impl AppState {
    pub fn new(
        root: TreeSource,
        kiro_root: PathBuf,
        size: (u16, u16),
        watch: WatchStatus,
        tree_mode: TreeMode,
        tree_visible: bool,
    ) -> Self {
        AppState {
            root,
            kiro_root,
            tree_mode,
            tree_visible,
            doc_focus: false,
            tree: TreeState::default(),
            sort_key: SortKey::default(),
            focus: Panel::Tree,
            doc: DocView::Empty,
            scroll: 0,
            search: SearchState::default(),
            tree_search: SearchState::default(),
            tree_matches: Vec::new(),
            popup: None,
            watch,
            size,
            layout: PanelLayout::default(),
            split: 0,
            drag: None,
            selection: None,
            auto_scroll: None,
            clipboard: Box::new(clipboard::Osc52),
            files_tree_cache: None,
        }
    }
}

/// Reducer outcome: whether the caller (`main.rs`, task 7.x) should keep
/// running the event loop or tear down the terminal and exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Continue,
    Quit,
    /// Requirement 1.1: `main` (task 2.x) consumes this to suspend the
    /// terminal and launch an editor against the given path. The reducer
    /// itself never touches the terminal or spawns a process -- it only
    /// decides *whether* an edit should happen (`current_editable_path`)
    /// and hands the path back for `main` to act on.
    EditFile(PathBuf),
    /// Requirement 1.4: `main` (a later task) consumes this to actually
    /// decide which source mode to switch to and restart the file watcher
    /// against it -- the same "reducer only signals intent" split
    /// `EditFile` already establishes for editing. The reducer itself never
    /// judges priority or touches the watcher; it always returns this,
    /// unconditionally, on `Action::ToggleSourceMode`.
    SwitchMode,
}

/// All inputs the reducer can react to.
pub enum Action {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Tick,
    Resize(u16, u16),
    Fs(crate::watch::FsEvent),
    Refresh,
    ToggleTree,
    /// Requirement 1.10: switch to one of the four layout modes. The single
    /// reducer entry point for every `TreeMode` transition (design.md "app
    /// — State & Reducer": "TreeMode 전이는 리듀서 한 곳") -- both the
    /// keymap's four hotkeys and any future programmatic caller go through
    /// this action rather than assigning `state.tree_mode` directly.
    SetTreeMode(TreeMode),
    Quit,
    /// Requirement 1.1: the edit key was pressed. Whether this actually
    /// starts an edit depends on what the doc panel is currently showing --
    /// see `current_editable_path`.
    Edit,
    /// Requirements 4.1 (editor command could not be launched) and 4.2
    /// (editor exited with a non-zero status): `main` (task 2.x) reports
    /// the failure back through this action after the terminal is restored,
    /// carrying the message to show the user.
    EditFailed(String),
    /// Requirement 1.4: the mode-switch key was pressed. The reducer never
    /// judges which mode to switch to or whether the switch is even
    /// possible -- see `Control::SwitchMode`'s doc comment -- it always
    /// signals intent and lets `main` act on it.
    ToggleSourceMode,
    /// `main`'s answer to a `Control::SwitchMode` signal once it has
    /// resolved the next mode and rebuilt the tree from disk (requirements
    /// 1.5, 1.6, 1.7): swap in the freshly built `source`, remember `root` as
    /// the new resync root, reset every piece of view state that keyed off
    /// the old tree/doc so nothing from the previous mode lingers, and
    /// record the outcome of restarting the file watcher against the new
    /// root (`watch_status`) so the status bar reflects the new mode's real
    /// watch state rather than the old one's.
    ApplySourceSwitch {
        source: TreeSource,
        root: PathBuf,
        watch_status: WatchStatus,
    },
    /// `main`'s answer to a `Control::SwitchMode` signal when the switch
    /// itself could not be completed (e.g. the target root failed to load)
    /// -- reported the same way `Action::EditFailed` reports an editor
    /// failure.
    SwitchModeFailed(String),
    /// Requirement 2.1/2.2: open every folder-like node in the current
    /// tree, leaving selection untouched. (Briefly merged with
    /// `CollapseAll` into a single toggle action post-`v0.6.0`; reverted
    /// back to two separate actions/keys per user request.)
    ExpandAll,
    /// Requirement 2.3/2.4: close every open node in the current tree,
    /// leaving selection untouched.
    CollapseAll,
}

/// The single state-transition entry point (design.md "app — State &
/// Reducer"). See `keymap.rs` for the key -> action-name table this
/// consults for `Action::Key`.
pub fn update(state: &mut AppState, action: Action) -> Control {
    match action {
        Action::Quit => Control::Quit,
        Action::Resize(w, h) => {
            resize_and_preserve_position(state, w, h);
            Control::Continue
        }
        Action::Fs(_) => {
            // Debounce/selective diffing is explicitly out of scope (design.md
            // "watch — FsWatcher"): any change under `.kiro` triggers one full
            // resync rather than inspecting which paths changed.
            resync(state);
            Control::Continue
        }
        Action::Refresh => {
            // Manual refresh (requirement 7.6) — same full resync as a live
            // Fs event, dispatched here for anything that sends the `Action`
            // directly rather than going through the `r`-key binding.
            resync(state);
            Control::Continue
        }
        Action::ToggleTree => {
            toggle_tree(state);
            Control::Continue
        }
        Action::SetTreeMode(mode) => {
            set_tree_mode(state, mode);
            Control::Continue
        }
        Action::Mouse(m) => handle_mouse(state, m),
        Action::Tick => {
            tick_auto_scroll(state);
            Control::Continue
        }
        Action::Key(key) => handle_key(state, key),
        Action::Edit => match current_editable_path(state) {
            Some(path) => Control::EditFile(path.to_path_buf()),
            None => {
                state.popup = Some(Popup::Message("편집할 파일이 없습니다".to_string()));
                Control::Continue
            }
        },
        Action::EditFailed(msg) => {
            state.popup = Some(Popup::Message(msg));
            Control::Continue
        }
        // Requirement 1.4: always signal intent -- `main` (a later task)
        // owns the actual priority judgment and watcher restart, exactly
        // as `Control::SwitchMode`'s doc comment describes.
        Action::ToggleSourceMode => Control::SwitchMode,
        Action::ApplySourceSwitch { source, root, watch_status } => {
            state.root = source;
            state.kiro_root = root;
            // Requirement 1.6: fold/selection state keys off the old tree's
            // `NodeId`s, which are meaningless against the new source.
            state.tree = TreeState::default();
            // Requirement 1.7: nothing is selected in the new tree yet, so
            // the doc panel has nothing left to show.
            state.doc = DocView::Empty;
            state.selection = None;
            state.search = SearchState::default();
            state.tree_search = SearchState::default();
            state.tree_matches = Vec::new();
            state.popup = None;
            // Requirement 1.5: the watch was restarted against the new root
            // before this action was dispatched (`main::handle_switch_mode`)
            // -- reflect its real outcome instead of leaving the old mode's
            // status behind.
            state.watch = watch_status;
            Control::Continue
        }
        Action::SwitchModeFailed(msg) => {
            state.popup = Some(Popup::Message(msg));
            Control::Continue
        }
        Action::ExpandAll => {
            expand_all(state);
            Control::Continue
        }
        Action::CollapseAll => {
            state.tree.close_all();
            Control::Continue
        }
    }
}

/// Requirement 2.1/2.2: open every folder-like node (`Spec`, `Dir` -- the
/// only `NodeId` variants that can have children; `Doc`/`File` are leaves
/// and opening them is meaningless) so every document becomes reachable
/// without manually expanding each ancestor. `Dir` covers both `--all`
/// mode's own folders and every `.kiro` group's root/subfolders
/// (spec-viewer-kiro-folder-groups) -- there is no separate "group root"
/// node kind to special-case. Reuses `search::flatten_tree`'s own traversal
/// (SSoT for "every node in this tree, in path order") rather than
/// re-walking `state.root` itself.
fn expand_all(state: &mut AppState) {
    for (path, _label) in search::flatten_tree(&state.root) {
        if matches!(path.last(), Some(NodeId::Spec(_)) | Some(NodeId::Dir(_))) {
            state.tree.open(path);
        }
    }
}

/// Requirements 1.1, 1.3: the file `Action::Edit` may open, if any. Only
/// `DocView::Rendered` names a real, currently-displayed file on disk --
/// every other variant (empty panel, a spec's definition summary, or one of
/// the four "can't show this doc" error states) has no single editable file
/// backing it, so `Action::Edit` must decline rather than guess.
fn current_editable_path(state: &AppState) -> Option<&Path> {
    match &state.doc {
        DocView::Rendered { path, .. } => Some(path),
        DocView::Empty
        | DocView::Definition { .. }
        | DocView::Missing(_)
        | DocView::Deleted(_)
        | DocView::ReadError { .. }
        | DocView::MetaError { .. } => None,
    }
}

/// Requirements 9.1 (click focus), 9.2 (wheel scroll under the pointer,
/// independent of focus), 9.4 (tree-row click selects + loads), and 9.5
/// (folder/twisty click also toggles). The sep/doc drag gestures (9.6, 9.7)
/// are tasks 12.4-12.5 -- a click on those regions here only moves focus.
fn handle_mouse(state: &mut AppState, m: MouseEvent) -> Control {
    match m.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // A fresh Down always starts (or explicitly declines) a new
            // gesture -- clear whatever the previous one left behind first,
            // in case its own Up was never delivered (e.g. released outside
            // the panel/terminal). Each arm below then sets whatever new
            // `drag` value actually applies.
            state.drag = None;
            state.auto_scroll = None;
            match mouse::panel_at(&state.layout, m.column, m.row) {
                Some(mouse::HitPanel::Tree) => {
                    state.focus = Panel::Tree;
                    state.selection = None; // 9.9: "다른 곳 클릭...으로 선택 해제"
                    handle_tree_click(state, m.column, m.row);
                }
                Some(mouse::HitPanel::Doc) => {
                    state.focus = Panel::Doc;
                    // Requirement 9.7: start a fresh selection at the
                    // clicked point (anchor == head until the pointer
                    // moves) rather than extending whatever selection was
                    // left over from a prior drag.
                    let pos = mouse::doc_position_clamped(state.layout.doc, state.scroll, m.column, m.row);
                    state.selection = Some(Selection { anchor: pos, head: pos });
                    state.drag = Some(Drag::Select);
                }
                Some(mouse::HitPanel::Sep) => {
                    state.selection = None; // 9.9: "다른 곳 클릭...으로 선택 해제"
                    state.drag = Some(Drag::Split);
                }
                None => {
                    state.selection = None; // 9.9: "다른 곳 클릭...으로 선택 해제"
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => match state.drag {
            Some(Drag::Split) => resize_split(state, m.column),
            Some(Drag::Select) => drag_select(state, m.column, m.row),
            None => {}
        },
        MouseEventKind::Up(MouseButton::Left) => {
            // Requirement 9.9: copy *before* clearing `drag` -- copying
            // needs to know a `Drag::Select` gesture (not e.g. `Split`) is
            // what just ended.
            if state.drag == Some(Drag::Select) {
                copy_selection(state);
            }
            state.drag = None;
            state.auto_scroll = None;
        }
        MouseEventKind::ScrollUp => scroll_panel_under_pointer(state, m.column, m.row, -3),
        MouseEventKind::ScrollDown => scroll_panel_under_pointer(state, m.column, m.row, 3),
        _ => {}
    }
    Control::Continue
}

/// Requirement 9.6 "패널 경계 좌클릭 드래그 → 트리/문서 폭 비율 조절, 최소 폭 20열
/// 보장": recompute `state.split` (the tree column's width) from the
/// pointer's current column, against the two-panel width `ui::render`
/// stored in `state.layout` on the *previous* frame (stable across a
/// single drag -- the main area's total width does not change mid-drag,
/// only how it is split). Clamped so both the tree and doc panel keep at
/// least [`MIN_PANEL_WIDTH`] columns; a no-op if the frame is currently too
/// narrow to honor that minimum on both sides at once (e.g. mid-resize to
/// a tiny terminal).
const MIN_PANEL_WIDTH: u16 = 20;

fn resize_split(state: &mut AppState, pointer_x: u16) {
    let total = state.layout.tree.width + state.layout.sep.width + state.layout.doc.width;
    if total < MIN_PANEL_WIDTH * 2 + 1 {
        return;
    }
    let raw = pointer_x.saturating_sub(state.layout.tree.x);
    let max_split = total - 1 - MIN_PANEL_WIDTH; // reserve the 1-col sep + doc's own minimum
    state.split = raw.clamp(MIN_PANEL_WIDTH, max_split);
}

/// Requirement 9.7: extend the active selection's `head` to wherever the
/// pointer has dragged to (clamped into the doc panel, per
/// `doc_position_clamped`), and arm/disarm `state.auto_scroll` depending on
/// whether the pointer currently sits at the panel's top/bottom edge --
/// `Action::Tick` (`tick_auto_scroll` below) is what actually advances the
/// scroll+selection one row at a time while it stays armed, since the
/// pointer can sit at the edge for many ticks without another `Drag` event
/// arriving (the mouse isn't moving, just held there).
fn drag_select(state: &mut AppState, x: u16, y: u16) {
    let Some(sel) = &mut state.selection else { return };
    sel.head = mouse::doc_position_clamped(state.layout.doc, state.scroll, x, y);
    state.auto_scroll = mouse::doc_edge_at(state.layout.doc, y).map(|edge| match edge {
        mouse::DocEdge::Top => AutoScrollDir::Up,
        mouse::DocEdge::Bottom => AutoScrollDir::Down,
    });
}

/// `Action::Tick` consumer for requirement 9.7's auto-scroll: while
/// `state.auto_scroll` is armed and a `Drag::Select` is still active,
/// scroll the doc panel one row in that direction and extend the
/// selection's `head` to match -- run once per tick, independent of any
/// new mouse event, so holding the pointer still at the edge keeps
/// scrolling.
fn tick_auto_scroll(state: &mut AppState) {
    if state.drag != Some(Drag::Select) {
        return;
    }
    let Some(dir) = state.auto_scroll else { return };
    let Some(sel) = &mut state.selection else { return };
    let delta: i64 = match dir {
        AutoScrollDir::Up => -1,
        AutoScrollDir::Down => 1,
    };
    let (line, col) = sel.head;
    sel.head = ((line as i64 + delta).max(0) as usize, col);
    scroll_by(state, delta);
}

/// Requirement 9.9: copy the current selection's text (design.md "선택
/// 텍스트(줄바꿈 포함)") to `state.clipboard`. A no-op if there is no
/// selection or the doc has no rendered plain text to slice.
///
/// Column offsets are treated as *character* indices into each line's
/// plain text -- a CJK/wide-glyph line's on-screen column position does
/// not line up column-for-column with its character index, the same
/// simplification `doc_panel`'s own search highlight already makes (see
/// its own comment) for the same reason: an exact mapping needs a full
/// grapheme-width walk this requirement's scope does not call for.
fn copy_selection(state: &mut AppState) {
    let Some(sel) = state.selection else { return };
    let Some(r) = rendered_of(&state.doc) else { return };
    let ((start_line, start_col), (end_line, end_col)) = sel.ordered();
    if start_line >= r.plain.len() {
        return;
    }
    let end_line = end_line.min(r.plain.len() - 1);

    let mut out = String::new();
    for line_idx in start_line..=end_line {
        if line_idx > start_line {
            out.push('\n');
        }
        let chars: Vec<char> = r.plain[line_idx].chars().collect();
        let from = if line_idx == start_line { start_col.min(chars.len()) } else { 0 };
        let to = if line_idx == end_line { end_col.min(chars.len()) } else { chars.len() };
        out.extend(&chars[from..to]);
    }
    state.clipboard.set(&out);
}

/// Requirements 9.4/9.5: resolve the row under `(x, y)` to the `NodeId`
/// path the tree widget actually rendered there (via
/// `mouse::tree_identifier_at`, itself a thin wrapper over
/// `TreeState::rendered_at` -- see that function's doc comment for why
/// tree-row math is not reimplemented here), select it and load it into
/// the doc panel exactly like the `Enter` key does (design.md's mouse
/// pseudocode: "트리 행이면 선택·문서 로드"), and additionally toggle it
/// open/closed when it is a folder node (`Spec`/`Dir` -- the only `NodeId`
/// variants with children) -- design.md: "폴더/▶▼ 이면 토글".
/// A coordinate outside every rendered row (`None`) is a no-op.
fn handle_tree_click(state: &mut AppState, x: u16, y: u16) {
    let Some(path) = mouse::tree_identifier_at(&state.tree, x, y).map(<[NodeId]>::to_vec) else {
        return;
    };
    state.tree.select(path.clone());
    load_selected_doc(state);
    if matches!(path.last(), Some(NodeId::Spec(_)) | Some(NodeId::Dir(_))) {
        state.tree.toggle(path);
    }
}

/// Requirement 9.2 "마우스 아래 패널이 3행 단위로 스크롤(트리는 선택 이동, 문서는
/// 본문 이동), 활성 패널이 아니어도 동작": scrolls whichever panel the pointer
/// is over, regardless of `state.focus`. The tree moves its *selection* (not
/// just the view offset) by `|delta_rows|` steps, one `key_up`/`key_down`
/// call per row -- matching "선택 이동" literally, the same way a real
/// arrow-key press would.
fn scroll_panel_under_pointer(state: &mut AppState, x: u16, y: u16, delta_rows: i64) {
    match mouse::panel_at(&state.layout, x, y) {
        Some(mouse::HitPanel::Tree) => {
            for _ in 0..delta_rows.unsigned_abs() {
                if delta_rows > 0 {
                    state.tree.key_down();
                } else {
                    state.tree.key_up();
                }
            }
        }
        Some(mouse::HitPanel::Doc) => scroll_by(state, delta_rows),
        Some(mouse::HitPanel::Sep) | None => {}
    }
}

/// Requirement 1.7 "실행 중 토글 키로 표시·숨김 전환": flip `tree_visible`,
/// except under `TreeMode::Always`, where the tree is pinned on and the
/// toggle key is a deliberate no-op (tasks.md 11.2 DONE text: "always 고정").
/// Hiding the tree while it is focused moves focus to the doc panel --
/// there is nothing left on screen to focus otherwise.
fn toggle_tree(state: &mut AppState) {
    if state.tree_mode == TreeMode::Always {
        return;
    }
    state.tree_visible = !state.tree_visible;
    if !state.tree_visible && state.focus == Panel::Tree {
        state.focus = Panel::Doc;
    }
}

/// Requirement 1.10: the single place every `TreeMode` transition goes
/// through. Each mode owns its own resulting `tree_visible`/`focus`
/// (Always/Auto pin the tree visible; Hidden hides it and moves focus off
/// if it was there; Single always (re)starts on the tree side). `doc_focus`
/// is reset to `false` on every transition -- it only has meaning while
/// already inside `Single`, so leaving or (re)entering the mode always
/// starts from the tree, never carrying over a stale "doc was showing" flag
/// from a previous Single session.
fn set_tree_mode(state: &mut AppState, mode: TreeMode) {
    state.tree_mode = mode;
    state.doc_focus = false;
    match mode {
        TreeMode::Always | TreeMode::Auto => {
            state.tree_visible = true;
        }
        TreeMode::Hidden => {
            state.tree_visible = false;
            if state.focus == Panel::Tree {
                state.focus = Panel::Doc;
            }
        }
        TreeMode::Single => {
            state.tree_visible = true;
            state.focus = Panel::Tree;
        }
    }
}

fn handle_key(state: &mut AppState, key: KeyEvent) -> Control {
    if state.popup.is_some() {
        // Modal: while any popup is open, only that popup's own keys apply
        // -- the global keymap action table below is not consulted at all.
        return handle_popup_key(state, key);
    }

    // Requirement 2.10 "Esc 로 해제": another global escape hatch, ahead of
    // the selection-clear check below -- a tree-panel search only ever
    // matters while the tree is focused, so this never fires against the
    // doc panel's own (unrelated) selection or search state.
    if key.code == KeyCode::Esc && state.focus == Panel::Tree && !state.tree_search.query.is_empty() {
        search::clear_tree_search(state);
        return Control::Continue;
    }

    // Requirement 9.9 "Esc 로 선택 해제": handled directly here, the same
    // way the popup-dismiss Esc above is -- a global escape hatch, not a
    // normal `keymap::BINDINGS` action.
    if key.code == KeyCode::Esc && state.selection.is_some() {
        state.selection = None;
        return Control::Continue;
    }

    // Requirement 1.10 "Esc -> 트리 복귀": another global escape hatch,
    // alongside the selection-clear one just above -- only meaningful while
    // `TreeMode::Single` is actually showing the doc panel.
    if key.code == KeyCode::Esc && state.tree_mode == TreeMode::Single && state.doc_focus {
        state.doc_focus = false;
        state.focus = Panel::Tree;
        return Control::Continue;
    }

    // Same "Esc -> 트리 복귀" rule for narrow Auto mode (requirement 1.10
    // "좁으면 단일 모드처럼 동작"), mirroring the `Single`-mode branch just
    // above -- narrow Auto has no separate `doc_focus` flag (it reuses
    // `focus` itself, see `load_selected_doc`), so the condition checks
    // `focus == Panel::Doc` directly instead.
    if key.code == KeyCode::Esc
        && state.tree_mode == TreeMode::Auto
        && state.size.0 < NARROW_WIDTH_THRESHOLD
        && state.focus == Panel::Doc
    {
        state.focus = Panel::Tree;
        return Control::Continue;
    }

    let Some(action_name) = keymap::action_for_key(key) else {
        return Control::Continue;
    };

    match action_name {
        "quit" => Control::Quit,
        "switch_panel" => {
            // 1.7: while the tree is hidden there is nothing to switch to --
            // focus stays pinned on the doc panel.
            if state.tree_visible {
                state.focus = match state.focus {
                    Panel::Tree => Panel::Doc,
                    Panel::Doc => Panel::Tree,
                };
            }
            Control::Continue
        }
        "toggle_tree" => {
            toggle_tree(state);
            Control::Continue
        }
        "layout_auto" => {
            set_tree_mode(state, TreeMode::Auto);
            Control::Continue
        }
        "layout_fold" => {
            set_tree_mode(state, TreeMode::Hidden);
            Control::Continue
        }
        "layout_expand" => {
            set_tree_mode(state, TreeMode::Always);
            Control::Continue
        }
        "layout_single" => {
            set_tree_mode(state, TreeMode::Single);
            Control::Continue
        }
        "cycle_sort" => {
            cycle_sort(state);
            Control::Continue
        }
        "help" => {
            state.popup = Some(Popup::Help(keymap::help_entries(), 0));
            Control::Continue
        }
        "select" => {
            load_selected_doc(state);
            Control::Continue
        }
        "line_down" => {
            scroll_by(state, 1);
            Control::Continue
        }
        "line_up" => {
            scroll_by(state, -1);
            Control::Continue
        }
        "half_page_down" => {
            scroll_by(state, half_page(state));
            Control::Continue
        }
        "half_page_up" => {
            scroll_by(state, -half_page(state));
            Control::Continue
        }
        "page_down" => {
            scroll_by(state, state.size.1 as i64);
            Control::Continue
        }
        "page_up" => {
            scroll_by(state, -(state.size.1 as i64));
            Control::Continue
        }
        "top" => {
            scroll_to(state, 0);
            Control::Continue
        }
        "bottom" => {
            // `scroll_to` clamps to the last line, so usize::MAX always
            // lands on the end.
            scroll_to(state, usize::MAX);
            Control::Continue
        }
        "prev_heading" => {
            jump_heading(state, false);
            Control::Continue
        }
        "next_heading" => {
            jump_heading(state, true);
            Control::Continue
        }
        // Requirement 9.3 "활성 패널에서 화살표 ↑↓ → 트리는 선택 이동, 문서는
        // 1행 스크롤": which panel these move depends on `state.focus`, not
        // always the tree (task 5.2's original tree-only binding, now
        // superseded now that a doc-focused panel exists to scroll).
        "nav_up" => {
            match state.focus {
                Panel::Tree => {
                    state.tree.key_up();
                }
                Panel::Doc => scroll_by(state, -1),
            }
            Control::Continue
        }
        "nav_down" => {
            match state.focus {
                Panel::Tree => {
                    state.tree.key_down();
                }
                Panel::Doc => scroll_by(state, 1),
            }
            Control::Continue
        }
        // Left/Right fold/unfold tree nodes -- meaningless on the doc
        // panel, so a no-op while it is focused rather than silently
        // mutating an unseen tree.
        "nav_left" => {
            if state.focus == Panel::Tree {
                state.tree.key_left();
            }
            Control::Continue
        }
        "nav_right" => {
            if state.focus == Panel::Tree {
                state.tree.key_right();
            }
            Control::Continue
        }
        // Requirement 2.10 "이전/다음 키는 문서 검색과 같은 키": which search
        // `n`/`N` cycles depends on which panel is focused, mirroring
        // `open_search`'s own routing below.
        "next_match" => {
            match state.focus {
                Panel::Tree => search::tree_next_match(state),
                Panel::Doc => search::next_match(state),
            }
            Control::Continue
        }
        "prev_match" => {
            match state.focus {
                Panel::Tree => search::tree_prev_match(state),
                Panel::Doc => search::prev_match(state),
            }
            Control::Continue
        }
        "refresh" => {
            resync(state);
            Control::Continue
        }
        "open_toc" => {
            // No-op when there is nothing to show (requirement 6.3 implies a
            // heading list; an empty/absent one is useless to pop up).
            let idx = rendered_of(&state.doc).and_then(|r| {
                if r.headings.is_empty() {
                    return None;
                }
                let scroll = state.scroll;
                Some(
                    r.headings
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(_, h)| h.line <= scroll)
                        .map(|(i, _)| i)
                        .unwrap_or(0),
                )
            });
            if let Some(idx) = idx {
                state.popup = Some(Popup::Toc(idx));
            }
            Control::Continue
        }
        "open_search" => {
            state.popup = Some(Popup::SearchInput(String::new()));
            Control::Continue
        }
        // The `'e'` binding (requirement 1.1/1.3): route through `update`
        // itself rather than duplicating `Action::Edit`'s
        // `current_editable_path`/popup logic here (SSoT) -- this is the
        // bridge from the keymap's `"edit"` string to that variant that a
        // real keypress needs; nothing else constructs `Action::Edit`.
        "edit" => update(state, Action::Edit),
        // The `'m'`/`'o'`/`'c'` bindings (requirements 1.4, 2.1, 2.2): same
        // bridge pattern as `"edit"` above -- task 1 defined
        // `Action::ToggleSourceMode`/`ExpandAll`/`CollapseAll` and task 2
        // bound the keys, but neither wired the keymap's action *names* to
        // these variants here, so a real keypress fell through to the
        // catch-all no-op below despite every unit test (which dispatches
        // the `Action` directly, never through `action_for_key`) passing --
        // caught only while building task 3.2's real end-to-end wiring, the
        // exact "dead wiring" class of bug this project has hit before with
        // `"edit"` itself.
        "toggle_source_mode" => update(state, Action::ToggleSourceMode),
        "expand_all" => update(state, Action::ExpandAll),
        "collapse_all" => update(state, Action::CollapseAll),
        _ => Control::Continue,
    }
}

/// Handle a key event while `state.popup` is `Some` (task 5.6). Each variant
/// owns its own key set; unrecognized keys are a no-op and the popup stays
/// open. Takes the popup by value (`Option::take`) to sidestep a double
/// mutable-borrow of `state` while still being able to read `state.doc`
/// (for `Toc`'s fresh `headings` lookup) and call `search::search` (for
/// `SearchInput`'s confirm) inside the match.
fn handle_popup_key(state: &mut AppState, key: KeyEvent) -> Control {
    match state.popup.take() {
        Some(Popup::Toc(selected)) => {
            let headings_len = rendered_of(&state.doc)
                .map(|r| r.headings.len())
                .unwrap_or(0);
            state.popup = match key.code {
                KeyCode::Down | KeyCode::Char('j') if headings_len > 0 => {
                    Some(Popup::Toc((selected + 1) % headings_len))
                }
                KeyCode::Up | KeyCode::Char('k') if headings_len > 0 => {
                    Some(Popup::Toc((selected + headings_len - 1) % headings_len))
                }
                KeyCode::Enter => {
                    if let Some(r) = rendered_of(&state.doc) {
                        if let Some(h) = r.headings.get(selected) {
                            state.scroll = h.line;
                        }
                    }
                    None
                }
                KeyCode::Esc => None,
                _ => Some(Popup::Toc(selected)),
            };
        }
        Some(Popup::SearchInput(mut buffer)) => {
            state.popup = match key.code {
                KeyCode::Char(c) => {
                    buffer.push(c);
                    Some(Popup::SearchInput(buffer))
                }
                KeyCode::Backspace => {
                    buffer.pop();
                    Some(Popup::SearchInput(buffer))
                }
                KeyCode::Enter => {
                    // Requirement 2.10: `/` routes to whichever panel was
                    // focused when the popup opened (`state.focus` is left
                    // untouched by typing into this popup -- see
                    // `handle_key`, which never changes it while a popup is
                    // open), the same way `next_match`/`prev_match` do.
                    match state.focus {
                        Panel::Tree => search::tree_search(state, &buffer),
                        Panel::Doc => search::search(state, &buffer),
                    }
                    None
                }
                KeyCode::Esc => None,
                _ => Some(Popup::SearchInput(buffer)),
            };
        }
        Some(Popup::Help(entries, selected)) => {
            let is_help_reopen_key = key.code == KeyCode::Char('?');
            let len = entries.len();
            state.popup = match key.code {
                _ if key.code == KeyCode::Esc || is_help_reopen_key => None,
                KeyCode::Down | KeyCode::Char('j') if len > 0 => {
                    Some(Popup::Help(entries, (selected + 1) % len))
                }
                KeyCode::Up | KeyCode::Char('k') if len > 0 => {
                    Some(Popup::Help(entries, (selected + len - 1) % len))
                }
                _ => Some(Popup::Help(entries, selected)),
            };
        }
        Some(Popup::Message(msg)) => {
            state.popup = if key.code == KeyCode::Esc {
                None
            } else {
                Some(Popup::Message(msg))
            };
        }
        None => {}
    }
    Control::Continue
}

/// Requirement 2.9 "정렬 키를 핫키로 순환": advance `state.sort_key` and
/// re-sort the current tree in place to match. `Kiro` cycles through all
/// four keys and re-sorts `root.specs`, exactly as before. `Files` (`--all`
/// mode, spec-viewer-files-mode-sort requirements 1.1-1.3, 2.1) cycles
/// through only `Name`/`Updated` (`SortKey::cycle_for_files` -- `Phase`/
/// `Progress` have no meaning without `.kiro` spec metadata) and
/// re-sorts the `FsTree`'s siblings via `FsTree::sort_entries`, which
/// preserves the tree's own nesting. `SpecKit` is a no-op -- it has no
/// sort concept, unaffected by this feature. In every case, `state.tree`'s
/// selected/opened sets are untouched (2.2): they key on `NodeId`, not
/// position, so reordering the underlying data never disturbs which node
/// is selected or open.
fn cycle_sort(state: &mut AppState) {
    match &mut state.root {
        TreeSource::Kiro(root) => {
            state.sort_key = state.sort_key.cycle();
            crate::spec::sort_specs(&mut root.specs, state.sort_key);
        }
        TreeSource::Files(tree) => {
            state.sort_key = state.sort_key.cycle_for_files();
            tree.sort_entries(state.sort_key);
        }
        TreeSource::SpecKit(_) => {}
    }
}

fn half_page(state: &AppState) -> i64 {
    (state.size.1 / 2) as i64
}

/// The `Rendered` payload of the currently displayed doc, if any — only
/// `DocView::Rendered` and `DocView::Definition` carry one (scroll/heading
/// navigation is a no-op for every other variant).
fn rendered_of(doc: &DocView) -> Option<&Rendered> {
    match doc {
        DocView::Rendered { r, .. } => Some(r),
        DocView::Definition { text, .. } => Some(text),
        _ => None,
    }
}

fn scroll_by(state: &mut AppState, delta: i64) {
    let Some(r) = rendered_of(&state.doc) else {
        return;
    };
    let max = r.plain.len().saturating_sub(1) as i64;
    let new = (state.scroll as i64 + delta).clamp(0, max);
    state.scroll = new as usize;
}

fn scroll_to(state: &mut AppState, target: usize) {
    let Some(r) = rendered_of(&state.doc) else {
        return;
    };
    let max = r.plain.len().saturating_sub(1);
    state.scroll = target.min(max);
}

fn jump_heading(state: &mut AppState, forward: bool) {
    let Some(r) = rendered_of(&state.doc) else {
        return;
    };
    if r.headings.is_empty() {
        return;
    }
    let scroll = state.scroll;
    if forward {
        if let Some(h) = r.headings.iter().find(|h| h.line > scroll) {
            state.scroll = h.line;
        }
    } else if let Some(h) = r.headings.iter().rev().find(|h| h.line < scroll) {
        state.scroll = h.line;
    }
}

/// Load whatever `state.tree`'s *current* selection resolves to into the
/// doc panel, resetting scroll and any running search — the shared body of
/// the `Enter`-key "select" action (requirement 2.6) and a tree-row mouse
/// click (requirement 9.4), so the two never drift apart.
fn load_selected_doc(state: &mut AppState) {
    let path = state.tree.selected().to_vec();
    let width = doc_panel_width(state);
    state.doc = resolve_selection(&state.root, &path, width);
    state.scroll = 0;
    search::clear_search(state);

    // Requirement 1.10 "문서 선택 -> 문서 전체 폭": only when something
    // actually loaded -- selecting a folder-like node (`Dir`, resolved to
    // `DocView::Empty`) has nothing to show, so Single mode stays on the
    // tree rather than switching to a blank doc panel.
    if state.tree_mode == TreeMode::Single {
        state.doc_focus = !matches!(state.doc, DocView::Empty);
        if state.doc_focus {
            state.focus = Panel::Doc;
        }
    } else if state.tree_mode == TreeMode::Auto && state.size.0 < NARROW_WIDTH_THRESHOLD {
        // Requirement 1.10 "자동... 좁으면 단일 모드처럼 동작": the width
        // fallback in `ui::render` already collapses narrow Auto to
        // whichever single panel `state.focus` names, but nothing was ever
        // flipping `focus` itself on selection the way Single mode's own
        // `doc_focus` above does -- so selecting a document while narrow
        // silently did nothing visible until the user manually pressed
        // Tab/Right (found from a user report: the expected behavior is
        // that narrow Auto mode auto-switches to the doc side on selection,
        // exactly like Single mode, not just "collapse to one panel and
        // require a manual switch").
        if !matches!(state.doc, DocView::Empty) {
            state.focus = Panel::Doc;
        }
    }
}

/// Resolve a tree-selection path (as returned by `TreeState::selected`) to
/// the `DocView` it should load (requirements 2.2, 2.6, 1.9). Generic over
/// `NodeId::Doc`'s position in `spec.docs` via `loader::load_for_selection`'s
/// own `.find()` — no hardcoded slot ordering here. `NodeId::File` now
/// occurs under both `TreeSource::Files` (`--all` mode) and
/// `TreeSource::Kiro` (a `.kiro` group's file, spec-viewer-kiro-folder-groups)
/// — both load the exact same way, straight from the path, regardless of
/// source. `NodeId::File` under `TreeSource::SpecKit` still can't happen in
/// practice (spec-kit has no group concept) but still resolves to `Empty`
/// rather than panic.
fn resolve_selection(root: &TreeSource, path: &[NodeId], width: u16) -> DocView {
    match (root, path.last()) {
        (_, None) => DocView::Empty,
        (TreeSource::Kiro(root), Some(NodeId::Spec(name))) => {
            match root.specs.iter().find(|s| &s.name == name) {
                Some(spec) => loader::load_for_selection(spec, None, width),
                None => DocView::Empty,
            }
        }
        (TreeSource::Kiro(root), Some(NodeId::Doc(name, kind))) => {
            match root.specs.iter().find(|s| &s.name == name) {
                Some(spec) => loader::load_for_selection(spec, Some(kind), width),
                None => DocView::Empty,
            }
        }
        // Requirement 1.9: a folder has no "정의" concept -- nothing to show.
        // Covers both `--all` mode's own folders and every `.kiro` group's
        // root/subfolders (there is no separate "group root" node kind).
        (_, Some(NodeId::Dir(_))) => DocView::Empty,
        (TreeSource::Files(_), Some(NodeId::File(path)))
        | (TreeSource::Kiro(_), Some(NodeId::File(path))) => loader::load_doc(path, width),
        // spec-kit reuses the same `NodeId::Spec`/`NodeId::Doc` kinds as
        // `.kiro` (task 3.2), and `Spec`/`DocEntry` are the shared domain
        // types, so this mirrors the `TreeSource::Kiro` arms above exactly,
        // just walking `features: &[Spec]` directly instead of `root.specs`.
        (TreeSource::SpecKit(features), Some(NodeId::Spec(name))) => {
            match features.iter().find(|s| &s.name == name) {
                Some(spec) => loader::load_for_selection(spec, None, width),
                None => DocView::Empty,
            }
        }
        (TreeSource::SpecKit(features), Some(NodeId::Doc(name, kind))) => {
            match features.iter().find(|s| &s.name == name) {
                Some(spec) => loader::load_for_selection(spec, Some(kind), width),
                None => DocView::Empty,
            }
        }
        // Mismatched (source, NodeId kind) combo that can't happen in
        // practice -- resolves to Empty rather than panic.
        (TreeSource::Files(_), Some(_)) | (TreeSource::SpecKit(_), Some(NodeId::File(_))) => {
            DocView::Empty
        }
    }
}

/// Re-render whatever `state.doc` currently shows at `width`, using its
/// *own* embedded identity (path, or spec name for a Definition/
/// MetaError) as the source -- not `state.tree`'s selection. Requirement
/// 1.6's file view never selects anything in the tree at all, so the old
/// tree-selection-based re-render (`resolve_selection`) always resolved an
/// empty selection to `DocView::Empty`, silently discarding the loaded
/// file on every resize or refresh (tasks.md 15.1's validate-impl finding).
/// This one function is now the single re-render path both
/// `resize_and_preserve_position` and `refresh_current_doc` use, so tree
/// view and file view can no longer diverge.
///
/// `None` only for `DocView::Empty` (nothing was ever loaded, so there is
/// nothing to reload) -- callers should leave `state.doc` untouched then,
/// rather than reassign it to itself.
fn reload_current_doc(state: &AppState, width: u16) -> Option<DocView> {
    match &state.doc {
        DocView::Empty => None,
        DocView::Rendered { path, .. }
        | DocView::Missing(path)
        | DocView::Deleted(path)
        | DocView::ReadError { path, .. } => Some(loader::load_doc(path, width)),
        DocView::Definition { spec, .. } | DocView::MetaError { spec, .. } => {
            let TreeSource::Kiro(root) = &state.root else {
                return None;
            };
            root.specs
                .iter()
                .find(|s| &s.name == spec)
                .map(|spec_ref| loader::load_definition(spec_ref, width))
        }
    }
}

/// Re-read `state.kiro_root` from disk and rebuild `state.root` in place
/// (requirements 7.3, 7.4: metadata/structure changes are picked up on any
/// `Action::Fs`/`Action::Refresh`), the same way for every `TreeSource`
/// (requirement 1.9's file-watch parity for `--all` mode, and 1.4's parity
/// for spec-kit mode). Debounce/selective diffing is explicitly out of
/// scope — this always does a full rebuild, which this crate's data sizes
/// make cheap.
///
/// For `TreeSource::SpecKit`, `state.kiro_root` holds the `specs/` watch
/// root (set by `main::resolve_source`, task 4) -- the same field `.kiro`
/// mode stores its own root in, just pointing at a different directory.
fn resync(state: &mut AppState) {
    state.root = match &state.root {
        TreeSource::Kiro(_) => {
            let snapshot = loader::load_snapshot(&state.kiro_root);
            let mut root = crate::spec::build(&snapshot);
            // Requirement 2.9: a rebuild from disk starts back at
            // `spec::build`'s own (filesystem-order) ordering -- re-apply
            // whatever sort the user had active so a live file change
            // (7.1-7.4) doesn't silently reset it.
            crate::spec::sort_specs(&mut root.specs, state.sort_key);
            TreeSource::Kiro(root)
        }
        TreeSource::Files(_) => {
            let mut tree = crate::spec::FsTree::scan(&state.kiro_root);
            // spec-viewer-files-mode-sort requirement 2.3: a rebuild from
            // disk starts back at `scan`'s own (path-order) ordering --
            // re-apply whatever sort the user had active, mirroring the
            // `Kiro` branch above.
            tree.sort_entries(state.sort_key);
            TreeSource::Files(tree)
        }
        TreeSource::SpecKit(_) => {
            TreeSource::SpecKit(crate::spec::spec_kit::build(&state.kiro_root))
        }
    };
    refresh_current_doc(state);
}

/// Re-resolve whatever is currently selected against the freshly rebuilt
/// `state.root`. Deliberately does not touch `state.tree`'s selected/opened
/// sets — that's what keeps the tree selection intact across a resync
/// (requirements 7.4, 7.5's "선택 유지").
fn refresh_current_doc(state: &mut AppState) {
    let old_doc_path: Option<PathBuf> = match &state.doc {
        DocView::Rendered { path, .. } => Some(path.clone()),
        _ => None,
    };

    if let Some(mut new_view) = reload_current_doc(state, doc_panel_width(state)) {
        // A doc that used to render successfully at exactly this path, and
        // now resolves as Missing, means it existed and was removed ->
        // Deleted (requirement 7.5), distinct from Missing's "never
        // generated" meaning.
        if let (DocView::Missing(p), Some(old)) = (&new_view, &old_doc_path) {
            if p == old {
                new_view = DocView::Deleted(p.clone());
            }
        }

        state.doc = new_view;
    }

    // Requirement 7.2: preserve scroll position; if the document shrank,
    // land on its new last line instead of out of bounds. Meaningless for
    // non-Rendered/Definition views (`rendered_of` returns None), left as-is.
    if let Some(r) = rendered_of(&state.doc) {
        let max = r.plain.len().saturating_sub(1);
        state.scroll = state.scroll.min(max);
    }
}

/// Handle `Action::Resize` (requirement 5.13: re-render at the new width
/// while keeping the current position "위치 유지"). A raw line number is
/// meaningless across a rewrap at a different width -- every line shifts --
/// so position is preserved by remembering which *heading* the top of the
/// view is currently under (by `(level, text)` identity, not line number),
/// re-rendering the current selection at the new width via `resolve_selection`
/// (the same helper `refresh_current_doc`/`resync` use), and then relocating
/// that same heading in the freshly rendered `headings` list.
fn resize_and_preserve_position(state: &mut AppState, w: u16, h: u16) {
    // Step 1: capture the current heading anchor (if any) before the resize.
    let anchor: Option<(u8, String)> = rendered_of(&state.doc).and_then(|r| {
        r.headings
            .iter()
            .rev()
            .find(|h| h.line <= state.scroll)
            .map(|h| (h.level, h.text.clone()))
    });

    // Step 2: record the new size (also used as the render width below).
    state.size = (w, h);

    // Step 3: re-render whatever is currently shown at the new width, via
    // the same path `refresh_current_doc`/`resync` use (requirement 1.6:
    // this must work for file view too, which has no tree selection at
    // all) -- no parallel re-wrap-without-disk-read logic here. `w` itself
    // is the *terminal's* new width, not the doc panel's -- `doc_panel_width`
    // accounts for the tree/sep columns a two-panel layout takes off of it
    // (tasks.md 20.1).
    if let Some(new_doc) = reload_current_doc(state, doc_panel_width(state)) {
        state.doc = new_doc;
    }

    // Step 4/5: relocate the anchor heading in the new render, or fall back
    // to clamping the existing scroll into the new valid range.
    match (&anchor, rendered_of(&state.doc)) {
        (Some((level, text)), Some(new_r)) => {
            state.scroll = new_r
                .headings
                .iter()
                .find(|h| h.level == *level && &h.text == text)
                .map(|h| h.line)
                .unwrap_or(0);
        }
        (None, Some(new_r)) => {
            let max = new_r.plain.len().saturating_sub(1);
            state.scroll = state.scroll.min(max);
        }
        (_, None) => {
            // Non-Rendered/Definition view -- scroll is meaningless, leave
            // it as-is (mirrors `refresh_current_doc`'s same handling).
        }
    }
}

#[cfg(test)]
mod reducer_tests {
    use super::*;
    use crate::markdown;
    use crate::spec::{self, DocKind, FsTree};
    use crossterm::event::KeyModifiers;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn fixtures_root() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/kiro"))
    }

    // --- resync (task 5.4) test helpers ---------------------------------
    //
    // These build a real, throwaway `.kiro`-shaped directory tree under the
    // system temp dir (never `tests/fixtures/`) so tests can mutate files on
    // disk mid-test and observe `resync`/`Action::Fs`/`Action::Refresh`
    // picking the change up.

    fn temp_kiro_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "spec_viewer_resync_{name}_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("specs")).unwrap();
        fs::create_dir_all(dir.join("steering")).unwrap();
        dir
    }

    fn write_spec(root: &Path, name: &str, phase: &str, requirements: &str) {
        let dir = root.join("specs").join(name);
        fs::create_dir_all(&dir).unwrap();
        let spec_json = format!(
            "{{\n  \"name\": \"{name}\",\n  \"created_at\": \"2026-01-01T00:00:00Z\",\n  \"updated_at\": \"2026-01-01T00:00:00Z\",\n  \"language\": \"ko\",\n  \"phase\": \"{phase}\",\n  \"approvals\": {{\n    \"requirements\": {{ \"generated\": true, \"approved\": true }}\n  }}\n}}\n"
        );
        fs::write(dir.join("spec.json"), spec_json).unwrap();
        fs::write(dir.join("requirements.md"), requirements).unwrap();
    }

    fn build_state_from(root: &Path, size: (u16, u16)) -> AppState {
        let snapshot = loader::load_snapshot(root);
        let spec_root = spec::build(&snapshot);
        AppState::new(
            TreeSource::Kiro(spec_root),
            root.to_path_buf(),
            size,
            WatchStatus::Live,
            TreeMode::Auto,
            true,
        )
    }

    fn fs_event() -> Action {
        Action::Fs(crate::watch::FsEvent { paths: vec![] })
    }

    fn test_state() -> AppState {
        let snapshot = loader::load_snapshot(&fixtures_root());
        let root = spec::build(&snapshot);
        AppState::new(
            TreeSource::Kiro(root),
            fixtures_root(),
            (120, 40),
            WatchStatus::Live,
            TreeMode::Auto,
            true,
        )
    }

    // --- task 20.1: `doc_panel_width` must predict `ui::render`'s real
    // split at every width class, not just the wide 2-panel case the
    // original bug report was filed against -------------------------------

    #[test]
    fn doc_panel_width_matches_full_width_when_tree_hidden() {
        for width in [40, 80, 120] {
            let mut state = test_state();
            state.size = (width, 40);
            state.tree_visible = false;
            assert_eq!(
                doc_panel_width(&state),
                width,
                "tree hidden: doc panel should always get the full terminal width at {width}"
            );
        }
    }

    #[test]
    fn doc_panel_width_matches_full_width_in_single_mode() {
        for width in [40, 80, 120] {
            let mut state = test_state();
            state.size = (width, 40);
            state.tree_mode = TreeMode::Single;
            assert_eq!(
                doc_panel_width(&state),
                width,
                "Single mode: doc panel should get the full main-area width at {width}"
            );
        }
    }

    #[test]
    fn doc_panel_width_matches_full_width_below_narrow_threshold() {
        // Below `NARROW_WIDTH_THRESHOLD` (80), `ui::render`'s own fallback
        // shows only the focused panel at the full main-area width --
        // there's no side-by-side split to shrink the doc panel by.
        let mut state = test_state();
        state.size = (40, 40);
        assert_eq!(doc_panel_width(&state), 40);
    }

    #[test]
    fn doc_panel_width_subtracts_tree_and_separator_at_two_panel_widths() {
        // At/above the threshold, `TreeMode::Auto` (this crate's default)
        // splits into `TREE_PANEL_PERCENT`% tree + 1-col separator + the
        // remainder for doc -- this is the split the original bug (tasks.md
        // 20's NO-GO) silently ignored, loading at the full terminal width
        // instead.
        for width in [80, 120] {
            let mut state = test_state();
            state.size = (width, 40);
            let expected_tree_width = (width as u32 * TREE_PANEL_PERCENT as u32 / 100) as u16;
            let expected_doc_width = width - expected_tree_width - 1;
            assert_eq!(
                doc_panel_width(&state),
                expected_doc_width,
                "2-panel at width {width}: doc panel should be the terminal width minus the \
                 tree column and its 1-column separator"
            );
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_action(state: &mut AppState, code: KeyCode) -> Control {
        update(state, Action::Key(key(code)))
    }

    /// A plausible two-panel `PanelLayout` (tree at x 0..30, sep at 30,
    /// doc at 31..120), matching `ui::render`'s own tree/sep/doc ordering
    /// closely enough for reducer-level mouse tests that don't need a real
    /// draw pass.
    fn two_panel_layout() -> PanelLayout {
        PanelLayout {
            tree: Rect::new(0, 0, 30, 39),
            sep: Rect::new(30, 0, 1, 39),
            doc: Rect::new(31, 0, 89, 39),
        }
    }

    fn mouse_action(kind: MouseEventKind, column: u16, row: u16) -> Action {
        Action::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn initial_tree_visible_logic() {
        assert!(initial_tree_visible(TreeMode::Always, true, 80));
        assert!(initial_tree_visible(TreeMode::Always, false, 80));
        assert!(!initial_tree_visible(TreeMode::Hidden, true, 80));
        assert!(!initial_tree_visible(TreeMode::Hidden, false, 80));
        assert!(!initial_tree_visible(TreeMode::Auto, true, 80));
        assert!(initial_tree_visible(TreeMode::Auto, false, 80));
    }

    #[test]
    fn toggle_tree_key_flips_visibility_under_auto_and_hidden() {
        let mut state = build_state_from(&fixtures_root(), (120, 40));
        state.tree_mode = TreeMode::Auto;
        assert!(state.tree_visible);
        update(&mut state, Action::Key(key(KeyCode::Char('T'))));
        assert!(!state.tree_visible);
        update(&mut state, Action::Key(key(KeyCode::Char('T'))));
        assert!(state.tree_visible);

        state.tree_mode = TreeMode::Hidden;
        state.tree_visible = false;
        update(&mut state, Action::ToggleTree);
        assert!(state.tree_visible);
    }

    #[test]
    fn toggle_tree_is_a_no_op_under_always() {
        let mut state = build_state_from(&fixtures_root(), (120, 40));
        state.tree_mode = TreeMode::Always;
        state.tree_visible = true;
        update(&mut state, Action::ToggleTree);
        assert!(
            state.tree_visible,
            "tasks.md 11.2 DONE: 'always 고정' -- the toggle key must not hide it"
        );
    }

    // --- task 19.1: layout mode hotkeys (requirements 1.10, 1.7, 8.5) ----

    #[test]
    fn layout_hotkeys_resolve_to_the_expected_tree_mode() {
        let mut state = build_state_from(&fixtures_root(), (120, 40));

        key_action(&mut state, KeyCode::Char('2'));
        assert_eq!(state.tree_mode, TreeMode::Hidden);
        assert!(!state.tree_visible);

        key_action(&mut state, KeyCode::Char('3'));
        assert_eq!(state.tree_mode, TreeMode::Always);
        assert!(state.tree_visible);

        key_action(&mut state, KeyCode::Char('4'));
        assert_eq!(state.tree_mode, TreeMode::Single);
        assert!(state.tree_visible);
        assert!(!state.doc_focus, "Single always (re)starts on the tree side");

        key_action(&mut state, KeyCode::Char('1'));
        assert_eq!(state.tree_mode, TreeMode::Auto);
        assert!(state.tree_visible);
    }

    #[test]
    fn set_tree_mode_action_matches_the_hotkey_behavior() {
        let mut state = build_state_from(&fixtures_root(), (120, 40));
        update(&mut state, Action::SetTreeMode(TreeMode::Single));
        assert_eq!(state.tree_mode, TreeMode::Single);
        assert!(state.tree_visible);
    }

    #[test]
    fn help_entries_include_all_four_layout_mode_keys() {
        let entries = keymap::help_entries();
        for (key, hint) in [("1", "자동"), ("2", "접기"), ("3", "펼치기"), ("4", "단일")] {
            assert!(
                entries.iter().any(|(k, h)| k == key && h.contains(hint)),
                "expected a help entry for {key:?} mentioning {hint:?}, got: {entries:?}"
            );
        }
    }

    #[test]
    fn single_mode_selecting_a_doc_switches_to_doc_full_width() {
        let mut state = build_state_from(&fixtures_root(), (120, 40));
        update(&mut state, Action::SetTreeMode(TreeMode::Single));
        assert!(!state.doc_focus);

        state.tree.select(vec![
            NodeId::Spec("sample-signup".to_string()),
            NodeId::Doc("sample-signup".to_string(), DocKind::Requirements),
        ]);
        key_action(&mut state, KeyCode::Enter);

        assert!(state.doc_focus, "selecting a real document should switch Single mode to the doc side");
        assert!(matches!(state.doc, DocView::Rendered { .. }));
    }

    #[test]
    fn single_mode_esc_returns_to_the_tree() {
        let mut state = build_state_from(&fixtures_root(), (120, 40));
        update(&mut state, Action::SetTreeMode(TreeMode::Single));
        state.tree.select(vec![
            NodeId::Spec("sample-signup".to_string()),
            NodeId::Doc("sample-signup".to_string(), DocKind::Requirements),
        ]);
        key_action(&mut state, KeyCode::Enter);
        assert!(state.doc_focus);

        key_action(&mut state, KeyCode::Esc);
        assert!(!state.doc_focus, "Esc should return Single mode to the tree side");
        assert_eq!(state.focus, Panel::Tree);
    }

    #[test]
    fn single_mode_selecting_an_empty_node_does_not_switch_to_doc() {
        // Requirement 1.9: a folder node (a `.kiro` group's root `Dir`)
        // resolves to DocView::Empty -- Single mode has nothing to show, so
        // it must stay on the tree rather than flipping to a blank doc
        // panel.
        let mut state = build_state_from(&fixtures_root(), (120, 40));
        update(&mut state, Action::SetTreeMode(TreeMode::Single));
        state.tree.select(vec![NodeId::Dir(fixtures_root().join("steering"))]);
        key_action(&mut state, KeyCode::Enter);

        assert!(!state.doc_focus);
        assert!(matches!(state.doc, DocView::Empty));
    }

    #[test]
    fn initial_tree_visible_is_true_for_single_regardless_of_file_arg() {
        assert!(initial_tree_visible(TreeMode::Single, true, 80));
        assert!(initial_tree_visible(TreeMode::Single, false, 80));
    }

    // --- bugfix (user report): requirement 1.10 "자동... 좁으면 단일
    // 모드처럼 동작" -- narrow Auto mode must also auto-switch to the doc
    // side on selection and return to the tree on Esc, exactly like Single
    // mode, not just collapse to whichever single panel `focus` already
    // names. Mirrors the `single_mode_*` tests just above one-for-one. -----

    #[test]
    fn narrow_auto_mode_selecting_a_doc_switches_focus_to_doc() {
        let mut state = build_state_from(&fixtures_root(), (40, 40));
        assert_eq!(state.tree_mode, TreeMode::Auto);
        assert_eq!(state.focus, Panel::Tree);

        state.tree.select(vec![
            NodeId::Spec("sample-signup".to_string()),
            NodeId::Doc("sample-signup".to_string(), DocKind::Requirements),
        ]);
        key_action(&mut state, KeyCode::Enter);

        assert_eq!(
            state.focus,
            Panel::Doc,
            "selecting a real document in narrow Auto mode should switch focus to the doc side"
        );
        assert!(matches!(state.doc, DocView::Rendered { .. }));
    }

    #[test]
    fn narrow_auto_mode_esc_returns_focus_to_the_tree() {
        let mut state = build_state_from(&fixtures_root(), (40, 40));
        state.tree.select(vec![
            NodeId::Spec("sample-signup".to_string()),
            NodeId::Doc("sample-signup".to_string(), DocKind::Requirements),
        ]);
        key_action(&mut state, KeyCode::Enter);
        assert_eq!(state.focus, Panel::Doc);

        key_action(&mut state, KeyCode::Esc);
        assert_eq!(state.focus, Panel::Tree, "Esc should return narrow Auto mode to the tree side");
    }

    #[test]
    fn narrow_auto_mode_selecting_an_empty_node_does_not_switch_focus() {
        // Requirement 1.9: a folder node (a `.kiro` group's root `Dir`)
        // resolves to DocView::Empty -- nothing to show, so focus must stay
        // on the tree rather than flipping to a blank doc panel.
        let mut state = build_state_from(&fixtures_root(), (40, 40));
        state.tree.select(vec![NodeId::Dir(fixtures_root().join("steering"))]);
        key_action(&mut state, KeyCode::Enter);

        assert_eq!(state.focus, Panel::Tree);
        assert!(matches!(state.doc, DocView::Empty));
    }

    #[test]
    fn wide_auto_mode_selecting_a_doc_does_not_auto_switch_focus() {
        // Requirement 1.10's auto-switch is specific to the narrow ("좁으면
        // 단일 모드처럼 동작") case -- at/above the threshold both panels
        // already show side by side, so there is nothing to "switch" to.
        let mut state = build_state_from(&fixtures_root(), (120, 40));
        assert_eq!(state.tree_mode, TreeMode::Auto);

        state.tree.select(vec![
            NodeId::Spec("sample-signup".to_string()),
            NodeId::Doc("sample-signup".to_string(), DocKind::Requirements),
        ]);
        key_action(&mut state, KeyCode::Enter);

        assert_eq!(
            state.focus,
            Panel::Tree,
            "wide Auto mode should leave focus alone on selection -- both panels are already visible"
        );
        assert!(matches!(state.doc, DocView::Rendered { .. }));
    }

    // --- task 19.2: spec tree sort (requirement 2.9) ---------------------

    fn spec_names(state: &AppState) -> Vec<String> {
        match &state.root {
            TreeSource::Kiro(root) => root.specs.iter().map(|s| s.name.clone()).collect(),
            TreeSource::SpecKit(_) | TreeSource::Files(_) => panic!("expected TreeSource::Kiro"),
        }
    }

    #[test]
    fn sort_hotkey_cycles_through_all_four_keys_and_reorders_specs() {
        // `AppState::new` itself does not sort (only an explicit
        // `Action::SetTreeMode`-style trigger does -- here, the `s`
        // hotkey/`cycle_sort`), so `root.specs` starts in `spec::build`'s
        // own filesystem-listing order, not alphabetical.
        let mut state = build_state_from(&fixtures_root(), (120, 40));
        assert_eq!(state.sort_key, spec::SortKey::Name);

        key_action(&mut state, KeyCode::Char('s'));
        assert_eq!(state.sort_key, spec::SortKey::Phase);
        let by_phase = spec_names(&state);
        let mut expected = by_phase.clone();
        expected.sort();
        assert_ne!(by_phase, expected, "sanity: fixture specs are not already in name order");

        key_action(&mut state, KeyCode::Char('s'));
        assert_eq!(state.sort_key, spec::SortKey::Updated);
        key_action(&mut state, KeyCode::Char('s'));
        assert_eq!(state.sort_key, spec::SortKey::Progress);
        key_action(&mut state, KeyCode::Char('s'));
        assert_eq!(state.sort_key, spec::SortKey::Name);
        assert_eq!(
            spec_names(&state),
            expected,
            "cycling back to Name should re-sort alphabetically"
        );
    }

    #[test]
    fn cycling_sort_preserves_the_current_tree_selection() {
        let mut state = build_state_from(&fixtures_root(), (120, 40));
        state.tree.select(vec![NodeId::Spec("sample-signup".to_string())]);

        key_action(&mut state, KeyCode::Char('s'));
        key_action(&mut state, KeyCode::Char('s'));

        assert_eq!(
            state.tree.selected(),
            &[NodeId::Spec("sample-signup".to_string())][..],
            "selection is keyed by NodeId, not position, so re-sorting the \
             underlying Vec must not change or clear it"
        );
    }

    #[test]
    fn sort_survives_a_resync() {
        let root = temp_kiro_root("sort_survives_resync");
        write_spec(&root, "z-spec", "discovery", "# req\n");
        write_spec(&root, "a-spec", "tasks", "# req\n");
        let mut state = build_state_from(&root, (120, 40));

        key_action(&mut state, KeyCode::Char('s')); // Name -> Phase
        assert_eq!(state.sort_key, spec::SortKey::Phase);
        assert_eq!(
            spec_names(&state),
            vec!["z-spec", "a-spec"],
            "\"discovery\" < \"tasks\" alphabetically"
        );

        update(&mut state, fs_event());

        assert_eq!(state.sort_key, spec::SortKey::Phase, "a resync must not reset the active sort key");
        assert_eq!(
            spec_names(&state),
            vec!["z-spec", "a-spec"],
            "a resync's freshly rebuilt SpecRoot must be re-sorted, not left in filesystem order"
        );
    }

    // --- Files-mode sort (spec-viewer-files-mode-sort) --------------------

    fn temp_files_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("spec_viewer_files_sort_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn build_files_state_from(root: &Path, size: (u16, u16)) -> AppState {
        let tree = crate::spec::FsTree::scan(root);
        AppState::new(TreeSource::Files(tree), root.to_path_buf(), size, WatchStatus::Live, TreeMode::Auto, true)
    }

    fn files_names(state: &AppState) -> Vec<String> {
        match &state.root {
            TreeSource::Files(tree) => tree
                .entries
                .iter()
                .map(|e| e.path.file_name().unwrap().to_string_lossy().into_owned())
                .collect(),
            TreeSource::Kiro(_) | TreeSource::SpecKit(_) => panic!("expected TreeSource::Files"),
        }
    }

    #[test]
    fn sort_hotkey_cycles_files_mode_between_name_and_updated_only() {
        let root = temp_files_root("cycle");
        fs::write(root.join("b.md"), "x").unwrap();
        fs::write(root.join("a.md"), "x").unwrap();
        let mut state = build_files_state_from(&root, (120, 40));
        assert_eq!(state.sort_key, spec::SortKey::Name);
        assert_eq!(files_names(&state), vec!["a.md", "b.md"]);

        key_action(&mut state, KeyCode::Char('s'));
        assert_eq!(
            state.sort_key,
            spec::SortKey::Updated,
            "Files mode's cycle must skip Phase/Progress entirely"
        );

        key_action(&mut state, KeyCode::Char('s'));
        assert_eq!(state.sort_key, spec::SortKey::Name);
        assert_eq!(files_names(&state), vec!["a.md", "b.md"]);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn cycling_sort_in_files_mode_preserves_the_current_tree_selection() {
        let root = temp_files_root("selection");
        fs::write(root.join("a.md"), "x").unwrap();
        fs::write(root.join("b.md"), "x").unwrap();
        let mut state = build_files_state_from(&root, (120, 40));
        let target = crate::spec::NodeId::File(root.join("b.md"));
        state.tree.select(vec![target.clone()]);

        key_action(&mut state, KeyCode::Char('s'));

        assert_eq!(
            state.tree.selected(),
            &[target][..],
            "selection is keyed by NodeId, not position, so re-sorting must not change or clear it"
        );

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn files_mode_sort_survives_a_resync() {
        let root = temp_files_root("resync");
        let now = std::time::SystemTime::now();
        fs::write(root.join("old.md"), "x").unwrap();
        std::fs::File::open(root.join("old.md"))
            .unwrap()
            .set_modified(now - std::time::Duration::from_secs(100))
            .unwrap();
        fs::write(root.join("new.md"), "x").unwrap();
        std::fs::File::open(root.join("new.md"))
            .unwrap()
            .set_modified(now + std::time::Duration::from_secs(100))
            .unwrap();

        let mut state = build_files_state_from(&root, (120, 40));
        key_action(&mut state, KeyCode::Char('s')); // Name -> Updated
        assert_eq!(state.sort_key, spec::SortKey::Updated);
        assert_eq!(files_names(&state), vec!["new.md", "old.md"]);

        update(&mut state, fs_event());

        assert_eq!(state.sort_key, spec::SortKey::Updated, "a resync must not reset the active sort key");
        assert_eq!(
            files_names(&state),
            vec!["new.md", "old.md"],
            "a resync's freshly rescanned FsTree must be re-sorted, not left in scan (path) order"
        );

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn spec_kit_mode_ignores_the_sort_hotkey() {
        // Requirement 4.2: spec-kit has no sort concept, unaffected by this
        // feature -- the key press is a harmless no-op.
        let root = temp_files_root("spec_kit_noop");
        let mut state = AppState::new(
            TreeSource::SpecKit(vec![]),
            root.clone(),
            (120, 40),
            WatchStatus::Live,
            TreeMode::Auto,
            true,
        );
        let before = state.sort_key;

        let control = key_action(&mut state, KeyCode::Char('s'));

        assert_eq!(control, Control::Continue);
        assert_eq!(state.sort_key, before);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn hiding_the_tree_while_it_is_focused_moves_focus_to_doc() {
        let mut state = build_state_from(&fixtures_root(), (120, 40));
        state.tree_mode = TreeMode::Auto;
        state.focus = Panel::Tree;
        update(&mut state, Action::ToggleTree);
        assert!(!state.tree_visible);
        assert_eq!(state.focus, Panel::Doc);
    }

    #[test]
    fn switch_panel_is_a_no_op_while_tree_is_hidden() {
        let mut state = build_state_from(&fixtures_root(), (120, 40));
        state.tree_visible = false;
        state.focus = Panel::Doc;
        update(&mut state, Action::Key(key(KeyCode::Tab)));
        assert_eq!(
            state.focus,
            Panel::Doc,
            "there is nothing to switch focus to while the tree is hidden"
        );
    }

    #[test]
    fn help_entries_include_toggle_tree_key() {
        let entries = keymap::help_entries();
        assert!(
            entries.iter().any(|(k, h)| k == "T" && h.contains("트리")),
            "expected a help entry for the tree-toggle key, got: {entries:?}"
        );
    }

    #[test]
    fn quit_action_returns_quit() {
        let mut state = test_state();
        assert_eq!(update(&mut state, Action::Quit), Control::Quit);
    }

    // --- task 12.2: click focus, wheel scroll, focused-panel arrows -----

    #[test]
    fn clicking_the_doc_panel_focuses_it() {
        let mut state = test_state();
        state.layout = two_panel_layout();
        state.focus = Panel::Tree;

        update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), 60, 5));
        assert_eq!(state.focus, Panel::Doc);
    }

    #[test]
    fn clicking_the_tree_panel_focuses_it() {
        let mut state = test_state();
        state.layout = two_panel_layout();
        state.focus = Panel::Doc;

        update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), 5, 5));
        assert_eq!(state.focus, Panel::Tree);
    }

    #[test]
    fn clicking_the_separator_does_not_change_focus() {
        let mut state = test_state();
        state.layout = two_panel_layout();
        state.focus = Panel::Tree;

        update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), 30, 5));
        assert_eq!(state.focus, Panel::Tree);
    }

    #[test]
    fn a_new_down_clears_a_stale_drag_left_over_from_an_undelivered_up() {
        // Regression (found while writing task 13.1's E2E flow): a Down in
        // the doc panel starts `Drag::Select`; if its matching Up is never
        // delivered (e.g. released outside the panel entirely) and the next
        // thing that happens is a Down elsewhere, the stale `Some(Select)`
        // must not survive to make a later, unrelated Up wrongly fire a
        // copy against whatever `selection` happens to exist by then.
        let mut state = doc_state_with_lines(20);
        update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), 40, 3));
        assert_eq!(state.drag, Some(Drag::Select));

        // A fresh Down on the tree, with no Up in between.
        update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), 5, 5));
        assert_eq!(
            state.drag, None,
            "a new Down on another panel must clear the previous gesture's leftover drag state"
        );
    }

    // --- task 12.4: sep drag resizes the split -------------------------

    #[test]
    fn down_on_sep_starts_a_split_drag() {
        let mut state = test_state();
        state.layout = two_panel_layout();
        assert_eq!(state.drag, None);

        update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), 30, 5));
        assert_eq!(state.drag, Some(Drag::Split));
    }

    #[test]
    fn dragging_after_sep_down_moves_the_split() {
        let mut state = test_state();
        state.layout = two_panel_layout();
        state.split = 30;

        update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), 30, 5));
        update(&mut state, mouse_action(MouseEventKind::Drag(MouseButton::Left), 50, 5));
        assert_eq!(state.split, 50);
    }

    #[test]
    fn dragging_without_a_prior_sep_down_does_nothing() {
        // A Drag event with no Down-on-sep first (e.g. the drag started
        // over the doc panel instead, or `drag` was cleared by Up already)
        // must leave the split alone.
        let mut state = test_state();
        state.layout = two_panel_layout();
        state.split = 30;
        state.drag = None;

        update(&mut state, mouse_action(MouseEventKind::Drag(MouseButton::Left), 80, 5));
        assert_eq!(state.split, 30);
    }

    #[test]
    fn dragging_clamps_to_the_minimum_panel_width_on_both_sides() {
        let mut state = test_state();
        state.layout = two_panel_layout(); // total width 120
        update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), 30, 5));

        // Far left: tree would shrink below 20 columns without the clamp.
        update(&mut state, mouse_action(MouseEventKind::Drag(MouseButton::Left), 2, 5));
        assert_eq!(state.split, 20);

        // Far right: doc would shrink below 20 columns without the clamp
        // (120 total - 1 sep - 20 min doc = 99 max split).
        update(&mut state, mouse_action(MouseEventKind::Drag(MouseButton::Left), 118, 5));
        assert_eq!(state.split, 99);
    }

    #[test]
    fn up_ends_the_drag_so_further_moves_are_ignored() {
        let mut state = test_state();
        state.layout = two_panel_layout();
        update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), 30, 5));
        update(&mut state, mouse_action(MouseEventKind::Up(MouseButton::Left), 30, 5));
        assert_eq!(state.drag, None);

        state.split = 30;
        update(&mut state, mouse_action(MouseEventKind::Drag(MouseButton::Left), 80, 5));
        assert_eq!(state.split, 30, "drag should have ended at Up");
    }

    // --- task 12.5: text-selection drag + edge auto-scroll --------------
    //
    // `two_panel_layout()`'s doc rect (Rect::new(31, 0, 89, 39)) has an
    // inner content area (1-cell border margin) of x in [32, 118], y in
    // [1, 37] -- the coordinates below are chosen against that.

    fn doc_state_with_lines(n: usize) -> AppState {
        let mut state = test_state();
        state.layout = two_panel_layout();
        state.doc = DocView::Rendered {
            path: PathBuf::from("inline.md"),
            r: markdown::render(&"line\n\n".repeat(n), 80),
            meta: FileInfo::default(),
        };
        state
    }

    #[test]
    fn mouse_down_on_doc_starts_a_selection_at_the_clicked_point() {
        let mut state = doc_state_with_lines(20);
        update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), 40, 3));
        assert_eq!(
            state.selection,
            Some(Selection { anchor: (2, 8), head: (2, 8) })
        );
        assert_eq!(state.drag, Some(Drag::Select));
    }

    #[test]
    fn dragging_extends_the_selection_head_but_not_the_anchor() {
        let mut state = doc_state_with_lines(20);
        update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), 40, 3));
        update(&mut state, mouse_action(MouseEventKind::Drag(MouseButton::Left), 50, 5));
        assert_eq!(
            state.selection,
            Some(Selection { anchor: (2, 8), head: (4, 18) })
        );
    }

    #[test]
    fn dragging_at_the_top_or_bottom_edge_arms_auto_scroll() {
        let mut state = doc_state_with_lines(20);
        update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), 40, 3));

        update(&mut state, mouse_action(MouseEventKind::Drag(MouseButton::Left), 40, 1));
        assert_eq!(state.auto_scroll, Some(AutoScrollDir::Up));

        update(&mut state, mouse_action(MouseEventKind::Drag(MouseButton::Left), 40, 37));
        assert_eq!(state.auto_scroll, Some(AutoScrollDir::Down));

        update(&mut state, mouse_action(MouseEventKind::Drag(MouseButton::Left), 40, 10));
        assert_eq!(state.auto_scroll, None, "back inside the panel should disarm auto-scroll");
    }

    #[test]
    fn tick_while_auto_scroll_armed_scrolls_and_extends_selection_up() {
        let mut state = doc_state_with_lines(20);
        state.scroll = 5;
        state.drag = Some(Drag::Select);
        state.auto_scroll = Some(AutoScrollDir::Up);
        state.selection = Some(Selection { anchor: (5, 0), head: (5, 0) });

        update(&mut state, Action::Tick);
        assert_eq!(state.scroll, 4);
        assert_eq!(state.selection.unwrap().head, (4, 0));
    }

    #[test]
    fn tick_while_auto_scroll_armed_scrolls_and_extends_selection_down() {
        let mut state = doc_state_with_lines(20);
        state.scroll = 5;
        state.drag = Some(Drag::Select);
        state.auto_scroll = Some(AutoScrollDir::Down);
        state.selection = Some(Selection { anchor: (5, 0), head: (5, 0) });

        update(&mut state, Action::Tick);
        assert_eq!(state.scroll, 6);
        assert_eq!(state.selection.unwrap().head, (6, 0));
    }

    #[test]
    fn tick_without_an_active_select_drag_does_nothing() {
        let mut state = doc_state_with_lines(20);
        state.scroll = 5;
        state.drag = None; // e.g. Up already fired, or a Split drag instead
        state.auto_scroll = Some(AutoScrollDir::Up);
        state.selection = Some(Selection { anchor: (5, 0), head: (5, 0) });

        update(&mut state, Action::Tick);
        assert_eq!(state.scroll, 5);
        assert_eq!(state.selection.unwrap().head, (5, 0));
    }

    #[test]
    fn up_ends_the_select_drag_but_keeps_the_selection_visible() {
        let mut state = doc_state_with_lines(20);
        update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), 40, 3));
        update(&mut state, mouse_action(MouseEventKind::Drag(MouseButton::Left), 50, 5));
        update(&mut state, mouse_action(MouseEventKind::Up(MouseButton::Left), 50, 5));

        assert_eq!(state.drag, None);
        assert_eq!(state.auto_scroll, None);
        assert_eq!(
            state.selection,
            Some(Selection { anchor: (2, 8), head: (4, 18) }),
            "the selection itself should survive Up -- task 12.6 clears it, not this"
        );
    }

    // --- task 12.6: copy on Up, clear on Esc/click-elsewhere -------------

    #[test]
    fn up_after_select_drag_copies_selected_text_including_newlines() {
        let mut state = test_state();
        state.layout = two_panel_layout();
        state.doc = DocView::Rendered {
            path: PathBuf::from("inline.md"),
            r: markdown::render("abcdefghij\n\nklmnopqrst\n\nuvwxyz0123\n", 80),
            meta: FileInfo::default(),
        };
        let sink = clipboard::TestSink::default();
        state.clipboard = Box::new(sink.clone());

        // Select from paragraph 0 col 2 ('c') to paragraph 2 col 4
        // (up to, not including, 'y'): three plain paragraphs
        // "abcdefghij" / "klmnopqrst" / "uvwxyz0123".
        update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), 34, 1));
        update(&mut state, mouse_action(MouseEventKind::Drag(MouseButton::Left), 36, 3));
        update(&mut state, mouse_action(MouseEventKind::Up(MouseButton::Left), 36, 3));

        assert_eq!(sink.last(), Some("cdefghij\nklmnopqrst\nuvwx".to_string()));
    }

    #[test]
    fn up_after_a_split_drag_does_not_copy_anything() {
        // Sanity: copying is specific to `Drag::Select` ending, not any
        // `Up` -- a boundary-resize drag (task 12.4) must not trigger it.
        let mut state = test_state();
        state.layout = two_panel_layout();
        let sink = clipboard::TestSink::default();
        state.clipboard = Box::new(sink.clone());

        update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), 30, 5));
        update(&mut state, mouse_action(MouseEventKind::Drag(MouseButton::Left), 50, 5));
        update(&mut state, mouse_action(MouseEventKind::Up(MouseButton::Left), 50, 5));

        assert_eq!(sink.last(), None);
    }

    #[test]
    fn clicking_elsewhere_clears_the_selection() {
        for click_at in [(5, 5), (30, 5)] {
            // (5,5) lands in the tree panel; (30,5) on the sep column --
            // both count as "다른 곳" relative to an existing doc selection.
            let mut state = doc_state_with_lines(20);
            state.selection = Some(Selection { anchor: (0, 0), head: (1, 1) });
            update(&mut state, mouse_action(MouseEventKind::Down(MouseButton::Left), click_at.0, click_at.1));
            assert_eq!(state.selection, None, "click at {click_at:?} should have cleared the selection");
        }
    }

    #[test]
    fn esc_clears_the_selection() {
        let mut state = doc_state_with_lines(20);
        state.selection = Some(Selection { anchor: (0, 0), head: (1, 1) });

        key_action(&mut state, KeyCode::Esc);
        assert_eq!(state.selection, None);
    }

    #[test]
    fn esc_with_no_selection_is_a_harmless_no_op() {
        let mut state = doc_state_with_lines(20);
        assert_eq!(state.selection, None);
        assert_eq!(key_action(&mut state, KeyCode::Esc), Control::Continue);
    }

    #[test]
    fn wheel_over_doc_scrolls_it_three_rows_regardless_of_focus() {
        let mut state = test_state();
        state.layout = two_panel_layout();
        state.focus = Panel::Tree; // requirement 9.2: "활성 패널이 아니어도 동작"
        state.doc = DocView::Rendered {
            path: PathBuf::from("inline.md"),
            r: markdown::render(&"line\n\n".repeat(20), 80),
            meta: FileInfo::default(),
        };
        state.scroll = 5;

        update(&mut state, mouse_action(MouseEventKind::ScrollDown, 60, 5));
        assert_eq!(state.scroll, 8);

        update(&mut state, mouse_action(MouseEventKind::ScrollUp, 60, 5));
        assert_eq!(state.scroll, 5);
    }

    #[test]
    fn wheel_over_tree_calls_key_down_the_right_number_of_times_regardless_of_focus() {
        // `TreeState::key_down`/`key_up` only move the selection once they
        // know the flattened item list, which is populated by a real
        // `Tree` widget render -- outside this reducer-only test's reach
        // (`app` must not depend on `ui`; see design.md's one-directional
        // layering). The selection-movement half of requirement 9.2 is
        // instead covered end-to-end, with a real render, by
        // `ui::tests::wheel_over_tree_moves_selection_regardless_of_focus`.
        // This test covers what *is* reachable here: the reducer resolves
        // `MouseEventKind::ScrollDown`/`ScrollUp` over the tree region to
        // exactly `|delta_rows|` `key_down`/`key_up` calls and nothing
        // else (in particular, it must not touch `state.scroll`).
        let mut state = test_state();
        state.layout = two_panel_layout();
        state.focus = Panel::Doc; // requirement 9.2: "활성 패널이 아니어도 동작"
        let scroll_before = state.scroll;

        // With an empty flattened list, `key_down`/`key_up` are no-ops on
        // the selection itself, but must still not panic and must leave
        // doc scroll untouched -- confirming the dispatch landed on the
        // tree branch, not the doc branch.
        update(&mut state, mouse_action(MouseEventKind::ScrollDown, 5, 5));
        assert_eq!(state.scroll, scroll_before);
        update(&mut state, mouse_action(MouseEventKind::ScrollUp, 5, 5));
        assert_eq!(state.scroll, scroll_before);
    }

    #[test]
    fn arrow_down_scrolls_doc_by_one_when_doc_is_focused() {
        let mut state = test_state();
        state.focus = Panel::Doc;
        state.doc = DocView::Rendered {
            path: PathBuf::from("inline.md"),
            r: markdown::render(&"line\n\n".repeat(20), 80),
            meta: FileInfo::default(),
        };
        state.scroll = 3;

        key_action(&mut state, KeyCode::Down);
        assert_eq!(state.scroll, 4);

        key_action(&mut state, KeyCode::Up);
        assert_eq!(state.scroll, 3);
    }

    #[test]
    fn arrow_down_does_not_scroll_doc_when_tree_is_focused() {
        // Complements `arrow_down_scrolls_doc_by_one_when_doc_is_focused`:
        // proves the reducer took the tree branch (see that test's sibling
        // `ui::tests::arrow_keys_move_tree_selection_when_tree_is_focused`
        // for the actual selection-movement half, which needs a real
        // render -- see the comment on
        // `wheel_over_tree_calls_key_down_the_right_number_of_times_regardless_of_focus`
        // above for why that can't happen in this reducer-only module).
        let mut state = test_state();
        state.focus = Panel::Tree;
        state.doc = DocView::Rendered {
            path: PathBuf::from("inline.md"),
            r: markdown::render(&"line\n\n".repeat(20), 80),
            meta: FileInfo::default(),
        };
        state.scroll = 3;

        key_action(&mut state, KeyCode::Down);
        assert_eq!(state.scroll, 3, "Down with Tree focused must not scroll the doc panel");
    }

    #[test]
    fn mouse_and_tick_actions_are_harmless_foundation_stubs() {
        // Task 12.1: the variants exist and are wired into `update` (real
        // click/wheel/drag/auto-scroll behavior is tasks 12.2-12.6) --
        // dispatching either must not panic or quit the app.
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

        let mut state = test_state();
        let before = state.focus;
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 5,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        assert_eq!(update(&mut state, Action::Mouse(mouse)), Control::Continue);
        assert_eq!(update(&mut state, Action::Tick), Control::Continue);
        assert_eq!(state.focus, before, "foundation stubs must not mutate state yet");
    }

    #[test]
    fn quit_key_returns_quit() {
        let mut state = test_state();
        assert_eq!(key_action(&mut state, KeyCode::Char('q')), Control::Quit);
    }

    #[test]
    fn switch_panel_key_toggles_focus() {
        let mut state = test_state();
        assert_eq!(state.focus, Panel::Tree);

        assert_eq!(key_action(&mut state, KeyCode::Tab), Control::Continue);
        assert_eq!(state.focus, Panel::Doc);

        assert_eq!(key_action(&mut state, KeyCode::Tab), Control::Continue);
        assert_eq!(state.focus, Panel::Tree);
    }

    #[test]
    fn help_key_opens_and_dismisses_help_popup() {
        let mut state = test_state();

        key_action(&mut state, KeyCode::Char('?'));
        match &state.popup {
            Some(Popup::Help(entries, selected)) => {
                assert!(entries.iter().any(|(k, h)| k == "q" && h.contains("종료")));
                assert!(entries.iter().any(|(_, h)| h.contains("전환")));
                assert!(entries.iter().any(|(k, _)| k == "["));
                assert!(entries.iter().any(|(k, _)| k == "]"));
                assert_eq!(*selected, 0, "help popup should open with the first row highlighted");
            }
            other => panic!("expected Popup::Help, got {other:?}"),
        }

        key_action(&mut state, KeyCode::Esc);
        assert_eq!(state.popup, None);

        // Re-opening and pressing the same `?` key again also dismisses.
        key_action(&mut state, KeyCode::Char('?'));
        assert!(state.popup.is_some());
        key_action(&mut state, KeyCode::Char('?'));
        assert_eq!(state.popup, None);
    }

    /// Regression for a real pty smoke test finding during
    /// `spec-viewer-tree-navigation-modes` verification: the help popup's
    /// `List` doesn't fit all 34 bindings in a realistically sized terminal,
    /// so `j`/`k`/Down/Up must move the highlighted row (letting
    /// `render_help`'s stateful `List` auto-scroll it into view) rather than
    /// being ignored the way every other popup key used to be.
    #[test]
    fn help_popup_down_and_up_move_the_highlighted_row_and_wrap() {
        let mut state = test_state();
        key_action(&mut state, KeyCode::Char('?'));
        let len = match &state.popup {
            Some(Popup::Help(entries, 0)) => entries.len(),
            other => panic!("expected Popup::Help opened at row 0, got {other:?}"),
        };
        assert!(len > 2, "test assumes more than a couple of bindings exist");

        key_action(&mut state, KeyCode::Down);
        assert!(matches!(state.popup, Some(Popup::Help(_, 1))));

        key_action(&mut state, KeyCode::Char('j'));
        assert!(matches!(state.popup, Some(Popup::Help(_, 2))));

        key_action(&mut state, KeyCode::Char('k'));
        assert!(matches!(state.popup, Some(Popup::Help(_, 1))));

        key_action(&mut state, KeyCode::Up);
        assert!(matches!(state.popup, Some(Popup::Help(_, 0))));

        // Wraps in both directions.
        key_action(&mut state, KeyCode::Up);
        match &state.popup {
            Some(Popup::Help(_, selected)) => assert_eq!(*selected, len - 1),
            other => panic!("expected Popup::Help, got {other:?}"),
        }
        key_action(&mut state, KeyCode::Down);
        assert!(matches!(state.popup, Some(Popup::Help(_, 0))));

        // Esc still closes it regardless of scroll position.
        key_action(&mut state, KeyCode::Down);
        key_action(&mut state, KeyCode::Esc);
        assert_eq!(state.popup, None);
    }

    #[test]
    fn select_key_loads_doc_node_into_rendered_view() {
        let mut state = test_state();
        state.tree.select(vec![
            NodeId::Spec("sample-signup".to_string()),
            NodeId::Doc("sample-signup".to_string(), DocKind::Requirements),
        ]);

        key_action(&mut state, KeyCode::Enter);

        match &state.doc {
            DocView::Rendered { path, .. } => {
                assert_eq!(path.file_name().unwrap(), "requirements.md");
            }
            other => panic!("expected Rendered, got {other:?}"),
        }
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn select_key_on_spec_node_loads_definition() {
        let mut state = test_state();
        state
            .tree
            .select(vec![NodeId::Spec("sample-signup".to_string())]);

        key_action(&mut state, KeyCode::Enter);

        assert!(matches!(state.doc, DocView::Definition { .. }));
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn select_key_on_stale_path_is_empty_without_panic() {
        let mut state = test_state();
        state
            .tree
            .select(vec![NodeId::Spec("does-not-exist".to_string())]);

        key_action(&mut state, KeyCode::Enter);

        assert!(matches!(state.doc, DocView::Empty));
    }

    /// Build a narrow-width rendered doc with enough lines to exercise
    /// scrolling, and load it directly into `state.doc` (bypassing the
    /// tree-selection path, which this test isn't about).
    fn load_wrapped_doc(state: &mut AppState) -> usize {
        let path = fixtures_root()
            .join("specs")
            .join("sample-signup")
            .join("requirements.md");
        let content = std::fs::read_to_string(&path).unwrap();
        let r = markdown::render(&content, 20);
        let len = r.plain.len();
        state.doc = DocView::Rendered { path, r, meta: FileInfo::default() };
        state.scroll = 0;
        len
    }

    #[test]
    fn line_scroll_moves_by_one_and_clamps_at_bounds() {
        let mut state = test_state();
        let len = load_wrapped_doc(&mut state);
        assert!(len > 2, "fixture doc too short to exercise scrolling: {len} lines");

        key_action(&mut state, KeyCode::Char('j'));
        assert_eq!(state.scroll, 1);
        key_action(&mut state, KeyCode::Char('k'));
        assert_eq!(state.scroll, 0);

        // Clamp at top.
        key_action(&mut state, KeyCode::Char('k'));
        assert_eq!(state.scroll, 0);

        // Clamp at bottom.
        key_action(&mut state, KeyCode::Char('G'));
        assert_eq!(state.scroll, len - 1);
        key_action(&mut state, KeyCode::Char('j'));
        assert_eq!(state.scroll, len - 1);

        key_action(&mut state, KeyCode::Char('g'));
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn heading_jump_moves_forward_then_back() {
        let mut state = test_state();
        state.doc = DocView::Rendered {
            path: PathBuf::from("inline.md"),
            r: markdown::render("# A\nbody\n# B\nbody2", 80),
            meta: FileInfo::default(),
        };
        state.scroll = 0;

        let first_line = match &state.doc {
            DocView::Rendered { r, .. } => r.headings[0].line,
            _ => unreachable!(),
        };
        let second_line = match &state.doc {
            DocView::Rendered { r, .. } => r.headings[1].line,
            _ => unreachable!(),
        };
        assert_eq!(state.scroll, first_line);

        key_action(&mut state, KeyCode::Char(']'));
        assert_eq!(state.scroll, second_line);

        key_action(&mut state, KeyCode::Char('['));
        assert_eq!(state.scroll, first_line);
    }

    #[test]
    fn resize_action_updates_size() {
        let mut state = test_state();
        assert_eq!(update(&mut state, Action::Resize(100, 30)), Control::Continue);
        assert_eq!(state.size, (100, 30));
    }

    // --- resize position preservation (task 5.5) ------------------------

    /// A long-enough body paragraph that wraps differently at width 120 vs
    /// 60, so the heading following it lands on a different line number at
    /// each width -- the thing task 5.5's anchor logic needs to survive.
    fn long_filler(label: &str) -> String {
        (0..8)
            .map(|i| {
                format!(
                    "{label} 문단 {i} 내용이 상당히 길게 이어지는 예시 텍스트입니다 lorem ipsum dolor sit amet consectetur adipiscing elit."
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn resize_preserves_position_at_current_heading_anchor() {
        let root = temp_kiro_root("resize_heading_anchor");
        let content = format!(
            "# Heading A\n\n{}\n\n# Heading B\n\n{}\n\n# Heading C\n\n{}\n",
            long_filler("A"),
            long_filler("B"),
            long_filler("C")
        );
        write_spec(&root, "resize-demo", "design", &content);

        let mut state = build_state_from(&root, (120, 40));
        state.tree.select(vec![
            NodeId::Spec("resize-demo".to_string()),
            NodeId::Doc("resize-demo".to_string(), DocKind::Requirements),
        ]);
        key_action(&mut state, KeyCode::Enter);

        // Jump from heading A (scroll starts at 0, its line) to heading B.
        key_action(&mut state, KeyCode::Char(']'));
        let (level_b, text_b, line_b_120) = match &state.doc {
            DocView::Rendered { r, .. } => {
                let h = &r.headings[1];
                assert_eq!(state.scroll, h.line);
                (h.level, h.text.clone(), h.line)
            }
            other => panic!("expected Rendered, got {other:?}"),
        };

        assert_eq!(update(&mut state, Action::Resize(60, 40)), Control::Continue);
        assert_eq!(state.size, (60, 40));

        let line_b_60 = match &state.doc {
            DocView::Rendered { r, .. } => r
                .headings
                .iter()
                .find(|h| h.level == level_b && h.text == text_b)
                .expect("heading B present after resize to width 60")
                .line,
            other => panic!("expected Rendered, got {other:?}"),
        };
        assert_eq!(state.scroll, line_b_60);
        assert_ne!(
            line_b_60, line_b_120,
            "expected different wrapping at width 60 vs 120 (test fixture not exercising a real rewrap)"
        );

        // Resize back to the original width: the anchor should land back on
        // the same heading's original line (rendering the same content at
        // the same width is deterministic).
        assert_eq!(update(&mut state, Action::Resize(120, 40)), Control::Continue);
        let line_b_120_again = match &state.doc {
            DocView::Rendered { r, .. } => r
                .headings
                .iter()
                .find(|h| h.level == level_b && h.text == text_b)
                .expect("heading B present after resize back to width 120")
                .line,
            other => panic!("expected Rendered, got {other:?}"),
        };
        assert_eq!(state.scroll, line_b_120_again);
        assert_eq!(line_b_120_again, line_b_120);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn resize_in_file_view_mode_keeps_the_document_instead_of_going_empty() {
        // Regression (validate-impl real-terminal check, tasks.md 15.1):
        // file view (`m path/x.md`, requirement 1.6) never puts anything
        // into `state.tree`'s selection -- `resize_and_preserve_position`
        // used to always re-render via that (tree-)selection path, which
        // resolves an empty selection to `DocView::Empty`, silently wiping
        // the loaded file out from under the user on every terminal resize.
        let root = temp_kiro_root("file_view_resize");
        let file_path = root.join("standalone.md");
        let content = format!(
            "# Heading A\n\n{}\n\n# Heading B\n\n{}\n",
            long_filler("A"),
            long_filler("B")
        );
        fs::write(&file_path, &content).unwrap();

        let snapshot = loader::load_snapshot(&root);
        let spec_root = spec::build(&snapshot);
        let mut state = AppState::new(
            TreeSource::Kiro(spec_root),
            root.clone(),
            (120, 40),
            WatchStatus::Live,
            TreeMode::Auto,
            false,
        );
        state.doc = loader::load_doc(&file_path, 120);
        assert_eq!(
            state.tree.selected(),
            Vec::<NodeId>::new(),
            "sanity: file view never selects anything in the tree"
        );

        let heading_b_line = match &state.doc {
            DocView::Rendered { r, .. } => r.headings[1].line,
            other => panic!("expected Rendered, got {other:?}"),
        };
        state.scroll = heading_b_line;

        update(&mut state, Action::Resize(60, 40));

        match &state.doc {
            DocView::Rendered { path, r, .. } => {
                assert_eq!(path, &file_path);
                let new_b_line = r.headings[1].line;
                assert_eq!(
                    state.scroll, new_b_line,
                    "expected the heading-B anchor to survive the resize"
                );
            }
            other => panic!("expected the file view's document to survive the resize, got {other:?}"),
        }

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn resize_with_no_heading_above_scroll_clamps_into_new_range() {
        let root = temp_kiro_root("resize_no_anchor");
        let intro = (0..5)
            .map(|i| format!("intro paragraph {i} lorem ipsum"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let content = format!("{intro}\n\n# Heading A\n\nbody a\n\n# Heading B\n\nbody b\n");
        write_spec(&root, "resize-no-anchor", "design", &content);

        let mut state = build_state_from(&root, (120, 40));
        state.tree.select(vec![
            NodeId::Spec("resize-no-anchor".to_string()),
            NodeId::Doc("resize-no-anchor".to_string(), DocKind::Requirements),
        ]);
        key_action(&mut state, KeyCode::Enter);
        assert_eq!(state.scroll, 0);
        // Sanity: scroll (0) sits before the first heading, so there is no
        // anchor to capture -- this exercises the "no anchor" clamp branch,
        // not the heading-relocation branch.
        match &state.doc {
            DocView::Rendered { r, .. } => assert!(
                r.headings[0].line > 0,
                "fixture too short to exercise the no-anchor branch"
            ),
            other => panic!("expected Rendered, got {other:?}"),
        }

        assert_eq!(update(&mut state, Action::Resize(60, 40)), Control::Continue);
        assert_eq!(state.size, (60, 40));
        match &state.doc {
            DocView::Rendered { r, .. } => {
                let max = r.plain.len().saturating_sub(1);
                assert!(state.scroll <= max);
                assert_eq!(state.scroll, 0);
            }
            other => panic!("expected Rendered, got {other:?}"),
        }

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn resize_on_empty_doc_does_not_panic() {
        let mut state = test_state();
        assert!(matches!(state.doc, DocView::Empty));

        assert_eq!(update(&mut state, Action::Resize(60, 40)), Control::Continue);

        assert_eq!(state.size, (60, 40));
        assert!(matches!(state.doc, DocView::Empty));
    }

    #[test]
    fn resize_while_viewing_definition_rerenders_without_panic() {
        // `## 정의` bodies are typically short/heading-free in practice, so
        // this confirms resize on a `DocView::Definition` re-renders at the
        // new width without crashing (falling into the "no anchor" clamp
        // branch via `rendered_of`, which already covers both `Rendered`
        // and `Definition`) rather than exercising heading relocation.
        let mut state = test_state();
        state
            .tree
            .select(vec![NodeId::Spec("sample-signup".to_string())]);
        key_action(&mut state, KeyCode::Enter);
        assert!(matches!(state.doc, DocView::Definition { .. }));

        assert_eq!(update(&mut state, Action::Resize(60, 40)), Control::Continue);

        assert_eq!(state.size, (60, 40));
        assert!(matches!(state.doc, DocView::Definition { .. }));
    }

    #[test]
    fn fs_event_resyncs_metadata_and_preserves_selection() {
        let root = temp_kiro_root("meta_change");
        write_spec(&root, "demo", "design", "# Demo\n\nbody\n");

        let mut state = build_state_from(&root, (120, 40));
        let sel = vec![NodeId::Spec("demo".to_string())];
        state.tree.select(sel.clone());
        key_action(&mut state, KeyCode::Enter);
        assert!(matches!(state.doc, DocView::Definition { .. }));

        // Edit spec.json on disk: phase design -> implementation.
        write_spec(&root, "demo", "implementation", "# Demo\n\nbody\n");

        assert_eq!(update(&mut state, fs_event()), Control::Continue);

        let spec = state
            .root
            .as_kiro()
            .expect("Kiro mode")
            .specs
            .iter()
            .find(|s| s.name == "demo")
            .expect("demo spec still present after resync");
        match &spec.kiro_meta {
            Some(Ok(meta)) => assert_eq!(meta.phase, "implementation"),
            Some(Err(e)) => panic!("expected Ok meta after resync, got {e}"),
            None => panic!("expected Some(kiro_meta) for a .kiro spec"),
        }
        assert_eq!(state.tree.selected().to_vec(), sel);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn fs_event_picks_up_newly_added_spec_directory() {
        let root = temp_kiro_root("structure_change");
        write_spec(&root, "first-spec", "design", "# First\n\nbody\n");

        let mut state = build_state_from(&root, (120, 40));
        assert_eq!(state.root.as_kiro().unwrap().specs.len(), 1);

        write_spec(&root, "second-spec", "design", "# Second\n\nbody\n");
        assert_eq!(update(&mut state, fs_event()), Control::Continue);

        assert_eq!(state.root.as_kiro().unwrap().specs.len(), 2);
        assert!(state.root.as_kiro().unwrap().specs.iter().any(|s| s.name == "second-spec"));

        fs::remove_dir_all(&root).unwrap();
    }

    // --- spec-viewer-spec-kit-support task 4: SpecKit resync + selection --

    fn temp_spec_kit_specs_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "spec_viewer_resync_spec_kit_{name}_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_spec_kit_feature(specs_dir: &Path, name: &str, spec_md: &str) {
        let dir = specs_dir.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("spec.md"), spec_md).unwrap();
    }

    fn build_spec_kit_state_from(specs_dir: &Path, size: (u16, u16)) -> AppState {
        let features = crate::spec::spec_kit::build(specs_dir);
        AppState::new(
            TreeSource::SpecKit(features),
            specs_dir.to_path_buf(),
            size,
            WatchStatus::Live,
            TreeMode::Auto,
            true,
        )
    }

    #[test]
    fn fs_event_picks_up_newly_added_spec_kit_feature_directory() {
        let specs_dir = temp_spec_kit_specs_dir("structure_change");
        write_spec_kit_feature(&specs_dir, "001-first", "# First\n");

        let mut state = build_spec_kit_state_from(&specs_dir, (120, 40));
        match &state.root {
            TreeSource::SpecKit(features) => assert_eq!(features.len(), 1),
            _ => panic!("expected TreeSource::SpecKit"),
        }

        write_spec_kit_feature(&specs_dir, "002-second", "# Second\n");
        assert_eq!(update(&mut state, fs_event()), Control::Continue);

        match &state.root {
            TreeSource::SpecKit(features) => {
                assert_eq!(features.len(), 2);
                assert!(features.iter().any(|s| s.name == "002-second"));
            }
            _ => panic!("expected TreeSource::SpecKit after resync"),
        }

        fs::remove_dir_all(&specs_dir).unwrap();
    }

    #[test]
    fn resolve_selection_spec_kit_doc_returns_rendered_view() {
        let specs_dir = temp_spec_kit_specs_dir("doc_selection");
        write_spec_kit_feature(&specs_dir, "001-login", "# Login Spec\n\nbody text\n");

        let mut state = build_spec_kit_state_from(&specs_dir, (120, 40));
        state.tree.select(vec![
            NodeId::Spec("001-login".to_string()),
            NodeId::Doc("001-login".to_string(), DocKind::Other("spec.md".to_string())),
        ]);
        key_action(&mut state, KeyCode::Enter);

        match &state.doc {
            DocView::Rendered { .. } => {}
            other => panic!("expected DocView::Rendered, got {other:?}"),
        }

        fs::remove_dir_all(&specs_dir).unwrap();
    }

    #[test]
    fn refresh_preserves_scroll_when_stable_and_clamps_when_content_shrinks() {
        let root = temp_kiro_root("scroll_clamp");
        let long = (0..30)
            .map(|i| format!("paragraph {i}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        write_spec(&root, "demo", "design", &long);

        let mut state = build_state_from(&root, (120, 40));
        state.tree.select(vec![
            NodeId::Spec("demo".to_string()),
            NodeId::Doc("demo".to_string(), DocKind::Requirements),
        ]);
        key_action(&mut state, KeyCode::Enter);
        let path = match &state.doc {
            DocView::Rendered { path, .. } => path.clone(),
            other => panic!("expected Rendered, got {other:?}"),
        };
        state.scroll = 10;

        // Non-shrinking edit (equal-or-longer content): scroll preserved
        // exactly, not reset.
        let longer = (0..40)
            .map(|i| format!("paragraph {i}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        fs::write(&path, &longer).unwrap();
        assert_eq!(update(&mut state, Action::Refresh), Control::Continue);
        assert_eq!(
            state.scroll, 10,
            "scroll should be preserved when content does not shrink"
        );

        // Shrinking edit: scroll clamps to the new last valid line instead
        // of staying out of bounds or resetting to 0.
        fs::write(&path, "only one short paragraph").unwrap();
        assert_eq!(update(&mut state, Action::Refresh), Control::Continue);
        let new_len = match &state.doc {
            DocView::Rendered { r, .. } => r.plain.len(),
            other => panic!("expected Rendered, got {other:?}"),
        };
        assert!(
            new_len <= 10,
            "expected shrink below previous scroll of 10, got {new_len} lines"
        );
        assert_eq!(state.scroll, new_len.saturating_sub(1));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn fs_event_marks_deleted_document_and_preserves_selection() {
        let root = temp_kiro_root("deleted_doc");
        write_spec(&root, "demo", "design", "# Demo\n\nbody text here\n");

        let mut state = build_state_from(&root, (120, 40));
        let sel = vec![
            NodeId::Spec("demo".to_string()),
            NodeId::Doc("demo".to_string(), DocKind::Requirements),
        ];
        state.tree.select(sel.clone());
        key_action(&mut state, KeyCode::Enter);
        let path = match &state.doc {
            DocView::Rendered { path, .. } => path.clone(),
            other => panic!("expected Rendered, got {other:?}"),
        };

        fs::remove_file(&path).unwrap();
        assert_eq!(update(&mut state, fs_event()), Control::Continue);

        match &state.doc {
            DocView::Deleted(p) => assert_eq!(p, &path),
            other => panic!("expected Deleted, got {other:?}"),
        }
        assert_eq!(state.tree.selected().to_vec(), sel);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn r_key_triggers_manual_refresh() {
        let root = temp_kiro_root("manual_refresh_key");
        write_spec(&root, "demo", "design", "# Demo\n\nbody\n");

        let mut state = build_state_from(&root, (120, 40));
        state
            .tree
            .select(vec![NodeId::Spec("demo".to_string())]);
        key_action(&mut state, KeyCode::Enter);

        write_spec(&root, "demo", "implementation", "# Demo\n\nbody\n");
        assert_eq!(key_action(&mut state, KeyCode::Char('r')), Control::Continue);

        let spec = state
            .root
            .as_kiro()
            .expect("Kiro mode")
            .specs
            .iter()
            .find(|s| s.name == "demo")
            .expect("demo spec still present after manual refresh");
        match &spec.kiro_meta {
            Some(Ok(meta)) => assert_eq!(meta.phase, "implementation"),
            Some(Err(e)) => panic!("expected Ok meta after manual refresh, got {e}"),
            None => panic!("expected Some(kiro_meta) for a .kiro spec"),
        }

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn fs_event_in_file_view_mode_reloads_the_file_instead_of_going_empty() {
        // Same root cause as `resize_in_file_view_mode_keeps_the_document_
        // instead_of_going_empty` (tasks.md 15.1), for the other caller of
        // the old tree-selection-based re-render: an `Action::Fs`/manual
        // refresh in file view (no tree selection at all) used to wipe the
        // loaded file to `DocView::Empty` too.
        let root = temp_kiro_root("file_view_fs_event");
        let file_path = root.join("standalone.md");
        fs::write(&file_path, "# Title\n\noriginal body\n").unwrap();

        let snapshot = loader::load_snapshot(&root);
        let spec_root = spec::build(&snapshot);
        let mut state = AppState::new(
            TreeSource::Kiro(spec_root),
            root.clone(),
            (120, 40),
            WatchStatus::Live,
            TreeMode::Auto,
            false,
        );
        state.doc = loader::load_doc(&file_path, 120);

        fs::write(&file_path, "# Title\n\nupdated body\n").unwrap();
        assert_eq!(update(&mut state, fs_event()), Control::Continue);

        match &state.doc {
            DocView::Rendered { path, r, .. } => {
                assert_eq!(path, &file_path);
                assert!(
                    r.plain.iter().any(|l| l.contains("updated body")),
                    "expected the file view to pick up the on-disk edit, got {:?}",
                    r.plain
                );
            }
            other => panic!("expected the file view's document to survive the fs event, got {other:?}"),
        }

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn fs_and_refresh_actions_resync_without_panicking_when_nothing_changed() {
        let root = temp_kiro_root("idempotent");
        write_spec(&root, "demo", "design", "# Demo\n\nbody\n");

        let mut state = build_state_from(&root, (120, 40));
        state
            .tree
            .select(vec![NodeId::Spec("demo".to_string())]);
        key_action(&mut state, KeyCode::Enter);

        assert_eq!(update(&mut state, fs_event()), Control::Continue);
        assert_eq!(update(&mut state, Action::Refresh), Control::Continue);
        assert_eq!(update(&mut state, fs_event()), Control::Continue);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tree_nav_actions_on_empty_tree_do_not_panic() {
        let mut state = test_state();
        assert_eq!(key_action(&mut state, KeyCode::Up), Control::Continue);
        assert_eq!(key_action(&mut state, KeyCode::Down), Control::Continue);
        assert_eq!(key_action(&mut state, KeyCode::Left), Control::Continue);
        assert_eq!(key_action(&mut state, KeyCode::Right), Control::Continue);
    }

    #[test]
    fn next_and_prev_match_keys_cycle_a_running_search() {
        let mut state = test_state();
        state.doc = DocView::Rendered {
            path: PathBuf::from("inline.md"),
            r: markdown::render("line one\n\nLINE TWO\n\nline three\n\nfour", 80),
            meta: FileInfo::default(),
        };
        state.scroll = 0;
        state.focus = Panel::Doc; // 2.10: n/N route to the doc search only while it's focused
        search::search(&mut state, "line"); // matches [0, 1, 2], current Some(0)

        key_action(&mut state, KeyCode::Char('n'));
        assert_eq!(state.search.current, Some(1));
        assert_eq!(state.scroll, 1);

        key_action(&mut state, KeyCode::Char('n'));
        assert_eq!(state.search.current, Some(2));
        assert_eq!(state.scroll, 2);

        key_action(&mut state, KeyCode::Char('N'));
        assert_eq!(state.search.current, Some(1));
        assert_eq!(state.scroll, 1);
    }

    // --- TOC popup + search-input popup (task 5.6) ----------------------

    fn load_headed_doc(state: &mut AppState) {
        state.doc = DocView::Rendered {
            path: PathBuf::from("inline.md"),
            r: markdown::render(
                "# Heading A\n\nbody a\n\n# Heading B\n\nbody b\n\n# Heading C\n\nbody c\n",
                80,
            ),
            meta: FileInfo::default(),
        };
        state.scroll = 0;
    }

    #[test]
    fn t_key_opens_toc_at_nearest_heading_at_or_before_scroll() {
        let mut state = test_state();
        load_headed_doc(&mut state);
        let heading_b_line = match &state.doc {
            DocView::Rendered { r, .. } => r.headings[1].line,
            other => panic!("expected Rendered, got {other:?}"),
        };
        state.scroll = heading_b_line;

        key_action(&mut state, KeyCode::Char('t'));

        match &state.popup {
            Some(Popup::Toc(idx)) => assert_eq!(*idx, 1),
            other => panic!("expected Some(Popup::Toc(1)), got {other:?}"),
        }
    }

    #[test]
    fn t_key_is_a_no_op_when_doc_has_no_headings() {
        let mut state = test_state();
        state.doc = DocView::Rendered {
            path: PathBuf::from("inline.md"),
            r: markdown::render("just a paragraph, no headings here", 80),
            meta: FileInfo::default(),
        };
        state.scroll = 0;

        key_action(&mut state, KeyCode::Char('t'));

        assert_eq!(state.popup, None);
    }

    #[test]
    fn t_key_is_a_no_op_on_empty_doc() {
        let mut state = test_state();
        assert!(matches!(state.doc, DocView::Empty));

        key_action(&mut state, KeyCode::Char('t'));

        assert_eq!(state.popup, None);
    }

    #[test]
    fn toc_move_wraps_and_confirms_jump() {
        let mut state = test_state();
        load_headed_doc(&mut state);

        key_action(&mut state, KeyCode::Char('t'));
        assert_eq!(state.popup, Some(Popup::Toc(0)));

        // Move forward twice: 0 -> 1 -> 2.
        key_action(&mut state, KeyCode::Char('j'));
        assert_eq!(state.popup, Some(Popup::Toc(1)));
        key_action(&mut state, KeyCode::Down);
        assert_eq!(state.popup, Some(Popup::Toc(2)));

        // Wrap forward past the last entry back to 0.
        key_action(&mut state, KeyCode::Char('j'));
        assert_eq!(state.popup, Some(Popup::Toc(0)));

        // Wrap backward past 0 back to the last entry.
        key_action(&mut state, KeyCode::Char('k'));
        assert_eq!(state.popup, Some(Popup::Toc(2)));
        key_action(&mut state, KeyCode::Up);
        assert_eq!(state.popup, Some(Popup::Toc(1)));

        let heading_line = match &state.doc {
            DocView::Rendered { r, .. } => r.headings[1].line,
            other => panic!("expected Rendered, got {other:?}"),
        };

        key_action(&mut state, KeyCode::Enter);
        assert_eq!(state.popup, None);
        assert_eq!(state.scroll, heading_line);
    }

    #[test]
    fn toc_esc_cancels_without_touching_scroll() {
        let mut state = test_state();
        load_headed_doc(&mut state);
        state.scroll = 0;

        key_action(&mut state, KeyCode::Char('t'));
        key_action(&mut state, KeyCode::Char('j'));
        key_action(&mut state, KeyCode::Char('j'));
        key_action(&mut state, KeyCode::Esc);

        assert_eq!(state.popup, None);
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn slash_key_opens_search_input_types_backspaces_and_confirms() {
        let mut state = test_state();
        state.doc = DocView::Rendered {
            path: PathBuf::from("inline.md"),
            r: markdown::render("line one\n\nLINE TWO\n\nline three\n\nfour", 80),
            meta: FileInfo::default(),
        };
        state.scroll = 0;
        state.focus = Panel::Doc; // 2.10: `/` routes to the doc search only while it's focused

        key_action(&mut state, KeyCode::Char('/'));
        assert_eq!(state.popup, Some(Popup::SearchInput(String::new())));

        for c in "linex".chars() {
            key_action(&mut state, KeyCode::Char(c));
        }
        assert_eq!(
            state.popup,
            Some(Popup::SearchInput("linex".to_string()))
        );

        key_action(&mut state, KeyCode::Backspace);
        assert_eq!(state.popup, Some(Popup::SearchInput("line".to_string())));

        key_action(&mut state, KeyCode::Enter);
        assert_eq!(state.popup, None);
        assert_eq!(state.search.query, "line");
        assert!(!state.search.matches.is_empty());
        assert_eq!(state.scroll, state.search.matches[0]);
    }

    #[test]
    fn search_input_esc_cancel_leaves_prior_confirmed_search_untouched() {
        let mut state = test_state();
        state.doc = DocView::Rendered {
            path: PathBuf::from("inline.md"),
            r: markdown::render("line one\n\nLINE TWO\n\nline three\n\nfour", 80),
            meta: FileInfo::default(),
        };
        state.scroll = 0;
        state.focus = Panel::Doc; // 2.10: `/` routes to the doc search only while it's focused
        search::search(&mut state, "line");
        let snapshot = state.search.clone();
        assert!(!snapshot.matches.is_empty(), "sanity: prior search matched");

        key_action(&mut state, KeyCode::Char('/'));
        for c in "four".chars() {
            key_action(&mut state, KeyCode::Char(c));
        }
        key_action(&mut state, KeyCode::Esc);

        assert_eq!(state.popup, None);
        assert_eq!(state.search, snapshot);
    }

    #[test]
    fn popup_open_is_modal_global_keys_are_no_ops_while_toc_or_search_open() {
        let mut state = test_state();
        load_headed_doc(&mut state);

        key_action(&mut state, KeyCode::Char('t'));
        assert!(matches!(state.popup, Some(Popup::Toc(_))));

        // `q` would normally quit; while the popup is open it's a no-op.
        assert_eq!(key_action(&mut state, KeyCode::Char('q')), Control::Continue);
        assert!(matches!(state.popup, Some(Popup::Toc(_))));

        // `/` would normally open the search popup; while Toc is open it's a
        // no-op (not consumed by the global keymap at all).
        key_action(&mut state, KeyCode::Char('/'));
        assert!(matches!(state.popup, Some(Popup::Toc(_))));

        key_action(&mut state, KeyCode::Esc);
        assert_eq!(state.popup, None);

        key_action(&mut state, KeyCode::Char('/'));
        assert_eq!(state.popup, Some(Popup::SearchInput(String::new())));

        // `q` would normally quit; while SearchInput is open, the global
        // keymap is not consulted at all, so `q` is simply typed into the
        // buffer instead of quitting -- `Control::Continue` is returned, not
        // `Control::Quit`, and the popup stays open.
        assert_eq!(key_action(&mut state, KeyCode::Char('q')), Control::Continue);
        assert_eq!(state.popup, Some(Popup::SearchInput("q".to_string())));

        // `t` would normally open the TOC popup; while SearchInput is open
        // it's likewise just typed into the buffer (not consumed globally).
        key_action(&mut state, KeyCode::Char('t'));
        assert_eq!(state.popup, Some(Popup::SearchInput("qt".to_string())));
    }

    #[test]
    fn help_popup_open_and_dismiss_still_works_via_popup_dispatch() {
        let mut state = test_state();

        key_action(&mut state, KeyCode::Char('?'));
        assert!(matches!(state.popup, Some(Popup::Help(_, _))));

        key_action(&mut state, KeyCode::Esc);
        assert_eq!(state.popup, None);

        key_action(&mut state, KeyCode::Char('?'));
        assert!(state.popup.is_some());
        key_action(&mut state, KeyCode::Char('?'));
        assert_eq!(state.popup, None);
    }

    #[test]
    fn selecting_a_new_doc_clears_prior_search_highlight() {
        let mut state = test_state();
        state.doc = DocView::Rendered {
            path: PathBuf::from("inline.md"),
            r: markdown::render("line one\n\nLINE TWO\n\nline three\n\nfour", 80),
            meta: FileInfo::default(),
        };
        state.scroll = 0;
        search::search(&mut state, "line");
        assert!(!state.search.matches.is_empty(), "sanity: search should have matched");

        state
            .tree
            .select(vec![NodeId::Spec("sample-billing".to_string())]);
        key_action(&mut state, KeyCode::Enter);

        assert_eq!(state.search, SearchState::default());
    }

    // --- task 19.4: tree panel search key routing (requirement 2.10) -----

    #[test]
    fn slash_key_with_tree_focused_runs_a_tree_search_not_a_doc_search() {
        let mut state = test_state();
        assert_eq!(state.focus, Panel::Tree, "sanity: tree is focused by default");

        key_action(&mut state, KeyCode::Char('/'));
        for c in "no-approvals".chars() {
            key_action(&mut state, KeyCode::Char(c));
        }
        key_action(&mut state, KeyCode::Enter);

        assert_eq!(state.popup, None);
        assert_eq!(state.tree_search.query, "no-approvals");
        assert_eq!(
            state.tree.selected(),
            [NodeId::Spec("no-approvals".to_string())]
        );
        // The doc search's own state is untouched by a tree-focused search.
        assert_eq!(state.search, SearchState::default());
    }

    #[test]
    fn n_and_shift_n_cycle_the_tree_search_while_tree_is_focused() {
        let mut state = test_state();
        search::tree_search(&mut state, "requirements.md");
        let total = state.tree_matches.len();
        assert!(total > 1, "fixture should have multiple requirements.md docs");

        key_action(&mut state, KeyCode::Char('n'));
        assert_eq!(state.tree_search.current, Some(1));
        assert_eq!(state.tree.selected(), state.tree_matches[1].as_slice());

        key_action(&mut state, KeyCode::Char('N'));
        assert_eq!(state.tree_search.current, Some(0));
        assert_eq!(state.tree.selected(), state.tree_matches[0].as_slice());
    }

    #[test]
    fn esc_clears_an_active_tree_search_while_tree_is_focused() {
        let mut state = test_state();
        search::tree_search(&mut state, "no-approvals");
        assert!(!state.tree_search.query.is_empty());

        key_action(&mut state, KeyCode::Esc);

        assert_eq!(state.tree_search, SearchState::default());
        assert!(state.tree_matches.is_empty());
        // The reveal/selection the search left behind survives the clear.
        assert_eq!(
            state.tree.selected(),
            [NodeId::Spec("no-approvals".to_string())]
        );
    }

    #[test]
    fn esc_with_no_active_tree_search_falls_through_to_other_esc_handling() {
        let mut state = test_state();
        state.selection = Some(Selection {
            anchor: (0, 0),
            head: (0, 1),
        });

        key_action(&mut state, KeyCode::Esc);

        // No tree search was running, so this Esc fell through to 9.9's
        // selection-clear instead of being swallowed as a no-op.
        assert_eq!(state.selection, None);
    }

    #[test]
    fn tree_search_no_match_shows_no_match_in_tree_search_state() {
        let mut state = test_state();

        search::tree_search(&mut state, "zzz-nonexistent");

        assert!(state.tree_search.matches.is_empty());
        assert_eq!(state.tree_search.current, None);
        assert_eq!(state.tree_search.query, "zzz-nonexistent");
    }

    // --- task 1.1: `Action::Edit` / `current_editable_path` ---------------

    /// Verify-completion regression (requirement 1.1): every other test in
    /// this section drives `Action::Edit` directly, which never proves the
    /// real `'e'` keypress -- `Action::Key(KeyCode::Char('e'))`, exactly what
    /// `run_loop` constructs from a live crossterm event -- actually reaches
    /// it through `handle_key`'s `keymap::action_for_key` string dispatch.
    /// It didn't (`"edit"` had no arm there, so it silently fell through to
    /// the wildcard `Control::Continue`) until this test caught it.
    #[test]
    fn e_keypress_reaches_action_edit_through_the_real_key_dispatch_path() {
        let mut state = test_state();
        let path = PathBuf::from("/tmp/spec-viewer-edit-test/requirements.md");
        state.doc = DocView::Rendered {
            path: path.clone(),
            r: markdown::render("# hi\n", 80),
            meta: FileInfo::default(),
        };

        let control = key_action(&mut state, KeyCode::Char('e'));

        assert_eq!(control, Control::EditFile(path));
    }

    #[test]
    fn edit_action_on_rendered_doc_returns_edit_file_control() {
        let mut state = test_state();
        let path = PathBuf::from("/tmp/spec-viewer-edit-test/requirements.md");
        state.doc = DocView::Rendered {
            path: path.clone(),
            r: markdown::render("# hi\n", 80),
            meta: FileInfo::default(),
        };

        let control = update(&mut state, Action::Edit);

        assert_eq!(control, Control::EditFile(path));
        assert_eq!(state.popup, None);
    }

    /// Requirement 1.3: every non-`Rendered` `DocView` state -- the empty
    /// panel, a spec's definition summary, and the four "can't show this
    /// doc" error states -- has no single file for `Action::Edit` to open,
    /// so all six must decline the same way: no `Control::EditFile`, and a
    /// status-bar message explaining why.
    #[test]
    fn edit_action_on_non_editable_doc_views_declines_with_a_status_message() {
        let doc_views: Vec<(&str, DocView)> = vec![
            ("Empty", DocView::Empty),
            (
                "Definition",
                DocView::Definition {
                    spec: "sample-signup".to_string(),
                    text: markdown::render("# def\n", 80),
                },
            ),
            ("Missing", DocView::Missing(PathBuf::from("missing.md"))),
            ("Deleted", DocView::Deleted(PathBuf::from("deleted.md"))),
            (
                "ReadError",
                DocView::ReadError {
                    path: PathBuf::from("unreadable.md"),
                    msg: "permission denied".to_string(),
                },
            ),
            (
                "MetaError",
                DocView::MetaError {
                    spec: "sample-signup".to_string(),
                    msg: "malformed spec.json".to_string(),
                },
            ),
        ];

        for (label, doc_view) in doc_views {
            let mut state = test_state();
            state.doc = doc_view;

            let control = update(&mut state, Action::Edit);

            assert_eq!(control, Control::Continue, "{label}: expected Continue");
            assert_eq!(
                state.popup,
                Some(Popup::Message("편집할 파일이 없습니다".to_string())),
                "{label}: expected the no-editable-file status message"
            );
        }
    }

    #[test]
    fn edit_failed_action_puts_the_message_straight_into_the_popup() {
        let mut state = test_state();

        let control = update(&mut state, Action::EditFailed("editor exited with error".to_string()));

        assert_eq!(control, Control::Continue);
        assert_eq!(
            state.popup,
            Some(Popup::Message("editor exited with error".to_string()))
        );
    }

    // --- Task 1.1/1.2 (spec-viewer-tree-navigation-modes): mode-switch
    // signal + expand/collapse-all --------------------------------------

    #[test]
    fn toggle_source_mode_action_always_returns_switch_mode_control() {
        // Requirement 1.4: the reducer never judges priority itself -- it
        // always signals `Control::SwitchMode` and leaves state untouched;
        // `main` (a later task) decides what actually happens.
        let mut state = test_state();
        state.tree.select(vec![NodeId::Spec("sample-signup".to_string())]);
        let selected_before = state.tree.selected().to_vec();

        let control = update(&mut state, Action::ToggleSourceMode);

        assert_eq!(control, Control::SwitchMode);
        assert_eq!(state.tree.selected(), selected_before.as_slice());
        assert!(state.popup.is_none());
    }

    #[test]
    fn apply_source_switch_replaces_root_and_resets_every_view_field() {
        // Requirements 1.6, 1.7: start from a state with something loaded
        // in every field that must be reset, so a missing reset actually
        // fails this test instead of vacuously passing.
        let mut state = test_state();
        state.tree.select(vec![NodeId::Spec("sample-signup".to_string())]);
        state.tree.open(vec![NodeId::Spec("sample-signup".to_string())]);
        state.doc = DocView::Rendered {
            path: PathBuf::from("/tmp/spec-viewer-switch-test.md"),
            r: markdown::render("# hi\n", 80),
            meta: FileInfo::default(),
        };
        state.selection = Some(Selection { anchor: (0, 0), head: (1, 2) });
        state.search = SearchState {
            query: "needle".to_string(),
            matches: vec![1, 2],
            current: Some(0),
        };
        state.tree_search = SearchState {
            query: "needle".to_string(),
            matches: vec![0],
            current: Some(0),
        };
        state.tree_matches = vec![vec![NodeId::Spec("sample-signup".to_string())]];
        state.popup = Some(Popup::Message("stale".to_string()));
        state.watch = WatchStatus::Live;

        let new_root = TreeSource::Files(FsTree { root: PathBuf::from("/tmp/new-root"), entries: vec![] });
        let new_kiro_root = PathBuf::from("/tmp/new-root");
        let control = update(
            &mut state,
            Action::ApplySourceSwitch {
                source: new_root,
                root: new_kiro_root.clone(),
                watch_status: WatchStatus::Manual { reason: "restarted".to_string() },
            },
        );

        assert_eq!(control, Control::Continue);
        assert!(matches!(state.root, TreeSource::Files(ref t) if t.root == PathBuf::from("/tmp/new-root")));
        assert_eq!(state.kiro_root, new_kiro_root);
        assert!(matches!(state.doc, DocView::Empty));
        assert!(state.tree.selected().is_empty());
        assert!(state.tree.opened().is_empty());
        assert_eq!(state.selection, None);
        assert_eq!(state.search, SearchState::default());
        assert_eq!(state.tree_search, SearchState::default());
        assert!(state.tree_matches.is_empty());
        assert_eq!(state.popup, None);
        // Requirement 1.5: the new watch outcome replaces the old one.
        assert_eq!(state.watch, WatchStatus::Manual { reason: "restarted".to_string() });
    }

    #[test]
    fn switch_mode_failed_action_puts_the_message_into_the_popup() {
        let mut state = test_state();

        let control = update(&mut state, Action::SwitchModeFailed("모드 전환 실패".to_string()));

        assert_eq!(control, Control::Continue);
        assert_eq!(state.popup, Some(Popup::Message("모드 전환 실패".to_string())));
    }

    /// Every `NodeId` variant that can hold children under `TreeSource::Kiro`
    /// (`Spec`, and every `.kiro` group's root `Dir`) must end up open; leaf
    /// nodes (`Doc`, `File`) are irrelevant to "expand all" and are not
    /// asserted on.
    #[test]
    fn expand_all_opens_every_folder_node_and_preserves_selection() {
        let mut state = test_state();
        let selected = vec![NodeId::Spec("sample-signup".to_string())];
        state.tree.select(selected.clone());
        assert!(state.tree.opened().is_empty(), "test setup: nothing should be open yet");

        let control = update(&mut state, Action::ExpandAll);

        assert_eq!(control, Control::Continue);
        for name in spec_names(&state) {
            assert!(
                state.tree.opened().contains(&vec![NodeId::Spec(name.clone())]),
                "expected Spec({name}) to be open after ExpandAll"
            );
        }
        assert!(state.tree.opened().contains(&vec![NodeId::Dir(fixtures_root().join("steering"))]));
        assert_eq!(state.tree.selected(), selected.as_slice());
    }

    #[test]
    fn collapse_all_closes_everything_and_preserves_selection() {
        let mut state = test_state();
        let selected = vec![NodeId::Spec("sample-signup".to_string())];
        state.tree.select(selected.clone());
        update(&mut state, Action::ExpandAll);
        assert!(!state.tree.opened().is_empty(), "test setup: expected something open");

        let control = update(&mut state, Action::CollapseAll);

        assert_eq!(control, Control::Continue);
        assert!(state.tree.opened().is_empty());
        assert_eq!(state.tree.selected(), selected.as_slice());
    }

    /// Verify-completion regression, same class as
    /// `e_keypress_reaches_action_edit_through_the_real_key_dispatch_path`:
    /// every test above drives `Action::ToggleSourceMode`/`ExpandAll`/
    /// `CollapseAll` directly, which never proves the real keypress --
    /// `Action::Key(KeyCode::Char('m'/'o'/'c'))`, exactly what `run_loop`
    /// constructs -- actually reaches these variants through `handle_key`'s
    /// `keymap::action_for_key` string dispatch. It didn't (none of the
    /// three action names had an arm there, so they silently fell through
    /// to the wildcard `Control::Continue`) until this test caught it.
    #[test]
    fn mode_switch_and_expand_collapse_keys_reach_their_actions_through_the_real_key_dispatch_path() {
        let mut state = test_state();

        assert_eq!(key_action(&mut state, KeyCode::Char('m')), Control::SwitchMode);

        assert!(state.tree.opened().is_empty());
        assert_eq!(key_action(&mut state, KeyCode::Char('o')), Control::Continue);
        assert!(!state.tree.opened().is_empty(), "expected 'o' to open at least one node");

        assert_eq!(key_action(&mut state, KeyCode::Char('c')), Control::Continue);
        assert!(state.tree.opened().is_empty(), "expected 'c' to close everything again");
    }

    #[test]
    fn expand_all_and_collapse_all_on_an_empty_tree_do_not_panic() {
        let mut state = AppState::new(
            TreeSource::Files(FsTree { root: PathBuf::from("/tmp/empty-files-root"), entries: vec![] }),
            PathBuf::from("/tmp/empty-files-root"),
            (120, 40),
            WatchStatus::Live,
            TreeMode::Auto,
            true,
        );

        assert_eq!(update(&mut state, Action::ExpandAll), Control::Continue);
        assert!(state.tree.opened().is_empty());
        assert_eq!(update(&mut state, Action::CollapseAll), Control::Continue);
        assert!(state.tree.opened().is_empty());
    }
}
