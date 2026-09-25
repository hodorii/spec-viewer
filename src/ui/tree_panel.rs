//! Tree panel: renders the spec/steering tree.
//!
//! Requirements: 2.1 (spec nodes), 2.2/2.3 (canonical doc order), 2.4
//! (missing-doc dimming), 2.5 (separate Steering group), 2.8 (steering
//! inclusion badge), 3.1 (phase badge), 3.2/3.4 (approval status symbols),
//! 3.5 (spec.json parse-failure warning badge), 4.1/4.3/4.4 (tasks
//! progress). Design.md "ui — Panels" `tree_panel`.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Block;
use ratatui::Frame;
use tui_tree_widget::{Tree, TreeItem, TreeState};

use crate::app::FilesTreeItemCache;
use crate::spec::{
    DocEntry, DocKind, DocStatus, FsEntry, FsTree, Inclusion, NodeId, SortKey, Spec, SpecRoot,
    SteeringDoc, TreeSource,
};

/// Style patched onto a search-matched node's label (requirement 2.10;
/// same "Yellow bg" convention `doc_panel`'s own search highlight uses for
/// a non-current match -- the *current* match additionally sits under the
/// tree's own selection, so it also gets the `Tree` widget's `REVERSED`
/// patch layered on top by `frame.render_stateful_widget` itself).
const SEARCH_MATCH_BG: Color = Color::Yellow;

/// Whether `path` (a node's own full identifier chain, root first) is one
/// of `search_matches` (requirement 2.10's "일치 노드 강조") -- an empty
/// `search_matches` (no running tree search) never matches anything.
fn is_search_match(path: &[NodeId], search_matches: &[Vec<NodeId>]) -> bool {
    search_matches.iter().any(|m| m.as_slice() == path)
}

/// Render the tree panel into `area` — dispatches on which [`TreeSource`]
/// is being browsed (requirement 1.8): the existing `.kiro`-schema tree, or
/// a plain markdown directory tree for `--all` mode. `sort_key` (requirement
/// 2.9) only affects the `Kiro` title -- the actual spec ordering is
/// `spec::sort_specs`'s job, applied to `SpecRoot.specs` before it ever
/// reaches this renderer; `Files` mode has no sort concept (requirement 1.9)
/// and ignores it. `search_matches` (requirement 2.10) is `state.tree_matches`
/// -- every node's full path a running tree search currently matches, each
/// one rendered with a distinguishing background; pass `&[]` when no tree
/// search is running. `files_tree_cache` is `Files`-mode's `Vec<TreeItem>`
/// build cache (spec-viewer-files-tree-scroll-latency) -- ignored under
/// `Kiro` (that mode's own item build stays uncached; see `render_kiro`'s
/// doc comment on why that scale doesn't need it).
pub fn render(
    frame: &mut Frame,
    area: Rect,
    root: &TreeSource,
    tree_state: &mut TreeState<NodeId>,
    sort_key: SortKey,
    search_matches: &[Vec<NodeId>],
    files_tree_cache: &mut Option<FilesTreeItemCache>,
) {
    match root {
        TreeSource::Kiro(root) => render_kiro(frame, area, root, tree_state, sort_key, search_matches),
        TreeSource::Files(tree) => {
            render_files(frame, area, tree, tree_state, search_matches, files_tree_cache)
        }
    }
}

/// Render the spec/steering tree into `area`.
///
/// A fresh `Vec<TreeItem<NodeId>>` is built on every call. This crate's
/// scale (a handful of specs/docs read from `.kiro/`) makes that cheap, so
/// no caching is attempted here.
fn render_kiro(
    frame: &mut Frame,
    area: Rect,
    root: &SpecRoot,
    tree_state: &mut TreeState<NodeId>,
    sort_key: SortKey,
    search_matches: &[Vec<NodeId>],
) {
    let mut items: Vec<TreeItem<'static, NodeId>> = root
        .specs
        .iter()
        .map(|spec| spec_item(spec, search_matches))
        .collect();
    // Steering group placed last, after every spec node (requirement 2.5
    // only requires it be a group separate from the specs — it does not
    // mandate a position — so "last" is a simple, stable choice).
    items.push(steering_group_item(&root.steering, search_matches));

    let title = format!("Specs [{}]", sort_key.label());
    let tree = Tree::new(&items)
        .expect("NodeId is unique per level by construction")
        .block(Block::bordered().title(title))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(tree, area, tree_state);
}

/// Render a `--all`-mode plain markdown directory tree (requirement 1.9):
/// folders fold/unfold, files are leaves, no badges/progress/definition
/// (those are `Kiro`-only concepts). `FsTree.entries` is flat with a
/// `depth` tag; [`build_files_items`] reconstructs the nested `TreeItem`
/// hierarchy `Tree` needs from it.
///
/// `cache` (spec-viewer-files-tree-scroll-latency) holds the last build:
/// reused as-is when `tree.entries`/`search_matches` are unchanged from
/// that build (the common case -- a scroll only moves the viewport, it
/// never adds/removes/renames entries or changes the active search), so a
/// large `--all` target no longer re-allocates the whole `TreeItem` tree
/// on every single render. `FsEntry`/`NodeId` are cheap (`PartialEq`, no
/// allocation) to compare; the full rebuild is what allocates a `String` +
/// `Line`/`Span`/`TreeItem` per entry, which is the actual cost this
/// avoids.
fn render_files(
    frame: &mut Frame,
    area: Rect,
    tree: &FsTree,
    tree_state: &mut TreeState<NodeId>,
    search_matches: &[Vec<NodeId>],
    cache: &mut Option<FilesTreeItemCache>,
) {
    let cache_hit = matches!(
        cache,
        Some((cached_entries, cached_matches, _))
            if cached_entries == &tree.entries && cached_matches.as_slice() == search_matches
    );
    if !cache_hit {
        let items = build_files_items(&tree.entries, search_matches);
        *cache = Some((tree.entries.clone(), search_matches.to_vec(), items));
    }
    let items = &cache.as_ref().expect("just populated above if it was empty").2;

    let widget = Tree::new(items)
        .expect("paths are unique per level by construction")
        .block(Block::bordered().title("Files"))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(widget, area, tree_state);
}

/// Rebuild the nested `TreeItem` hierarchy from `FsTree`'s flat,
/// depth-tagged, path-sorted entries. Each stack frame accumulates one
/// directory's not-yet-finalized children; a frame closes (and is appended
/// to its own parent's children) once an entry arrives whose depth is not
/// strictly deeper than the frame's own depth -- i.e. the entry can no
/// longer be that directory's descendant. `entries` being sorted by full
/// path (see `FsTree`'s own doc comment) is what guarantees a directory's
/// entire subtree arrives contiguously, right after the directory itself.
fn build_files_items(
    entries: &[FsEntry],
    search_matches: &[Vec<NodeId>],
) -> Vec<TreeItem<'static, NodeId>> {
    let mut stack: Vec<(u8, std::path::PathBuf, Vec<TreeItem<'static, NodeId>>)> =
        vec![(0, std::path::PathBuf::new(), Vec::new())];

    // The still-open ancestor `NodeId`s above whatever `stack` frame is
    // being closed/appended to right now -- `stack[1..]` (skipping the
    // depth-0 root sentinel), each frame's own path turned into its
    // `NodeId::Dir`. Recomputed on demand rather than tracked incrementally
    // since this tree is small and it is only needed at highlight-check
    // time (closing a dir, or appending a leaf).
    fn ancestors_of(
        stack: &[(u8, std::path::PathBuf, Vec<TreeItem<'static, NodeId>>)],
    ) -> Vec<NodeId> {
        stack[1..].iter().map(|(_, p, _)| NodeId::Dir(p.clone())).collect()
    }

    for entry in entries {
        while stack.len() > 1 && stack.last().unwrap().0 >= entry.depth {
            let (_, path, children) = stack.pop().unwrap();
            let mut own_path = ancestors_of(&stack);
            own_path.push(NodeId::Dir(path.clone()));
            let item = dir_item(&path, children, is_search_match(&own_path, search_matches));
            stack.last_mut().unwrap().2.push(item);
        }
        if entry.is_dir {
            stack.push((entry.depth, entry.path.clone(), Vec::new()));
        } else {
            let mut own_path = ancestors_of(&stack);
            own_path.push(NodeId::File(entry.path.clone()));
            let item = file_item(&entry.path, is_search_match(&own_path, search_matches));
            stack.last_mut().unwrap().2.push(item);
        }
    }

    while stack.len() > 1 {
        let (_, path, children) = stack.pop().unwrap();
        let mut own_path = ancestors_of(&stack);
        own_path.push(NodeId::Dir(path.clone()));
        let item = dir_item(&path, children, is_search_match(&own_path, search_matches));
        stack.last_mut().unwrap().2.push(item);
    }

    stack.pop().unwrap().2
}

fn file_name_label(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn labeled_line(label: String, highlighted: bool) -> Line<'static> {
    if highlighted {
        Line::from(Span::styled(label, Style::new().bg(SEARCH_MATCH_BG)))
    } else {
        Line::from(label)
    }
}

fn dir_item(
    path: &std::path::Path,
    children: Vec<TreeItem<'static, NodeId>>,
    highlighted: bool,
) -> TreeItem<'static, NodeId> {
    TreeItem::new(
        NodeId::Dir(path.to_path_buf()),
        labeled_line(file_name_label(path), highlighted),
        children,
    )
    .expect("directory paths are unique by construction")
}

fn file_item(path: &std::path::Path, highlighted: bool) -> TreeItem<'static, NodeId> {
    TreeItem::new_leaf(
        NodeId::File(path.to_path_buf()),
        labeled_line(file_name_label(path), highlighted),
    )
}

/// Build the `TreeItem` for one spec node and its document children.
fn spec_item(spec: &Spec, search_matches: &[Vec<NodeId>]) -> TreeItem<'static, NodeId> {
    let label = match &spec.meta {
        Ok(meta) => format!("{} [{}]", spec.name, meta.phase),
        // No phase is available when spec.json failed to parse (or is
        // missing) — requirement 3.5's warning badge.
        Err(_) => format!("{} !", spec.name),
    };

    let children: Vec<TreeItem<'static, NodeId>> = spec
        .docs
        .iter()
        .map(|entry| doc_item(&spec.name, entry, search_matches))
        .collect();

    let own_path = vec![NodeId::Spec(spec.name.clone())];
    let highlighted = is_search_match(&own_path, search_matches);

    TreeItem::new(NodeId::Spec(spec.name.clone()), labeled_line(label, highlighted), children)
        .expect("DocKind is unique within one spec's docs by construction")
}

/// Status-symbol legend from design.md's tree_panel spec:
/// `· 미생성 / ○ 미승인 / ● 승인 / ? 상태없음 / ! 경고`, with untracked
/// documents (`research`, `Other`) carrying no symbol at all.
fn status_symbol(status: DocStatus) -> &'static str {
    match status {
        DocStatus::Missing => "·",
        DocStatus::Generated => "○",
        DocStatus::Approved => "●",
        DocStatus::NoRecord => "?",
        DocStatus::NotTracked => " ",
    }
}

fn doc_filename(kind: &DocKind) -> String {
    match kind {
        DocKind::Requirements => "requirements.md".to_string(),
        DocKind::Bugfix => "bugfix.md".to_string(),
        DocKind::BizProcess => "biz-process.md".to_string(),
        DocKind::Design => "design.md".to_string(),
        DocKind::Tasks => "tasks.md".to_string(),
        DocKind::Research => "research.md".to_string(),
        DocKind::Other(name) => name.clone(),
    }
}

/// Build the `TreeItem` for one document node.
fn doc_item(spec_name: &str, entry: &DocEntry, search_matches: &[Vec<NodeId>]) -> TreeItem<'static, NodeId> {
    let symbol = status_symbol(entry.status);
    let filename = doc_filename(&entry.kind);

    // Requirement 2.4: a missing document keeps its row but dims it.
    let base_style = if entry.exists {
        Style::new()
    } else {
        Style::new().add_modifier(Modifier::DIM)
    };

    let own_path = vec![
        NodeId::Spec(spec_name.to_string()),
        NodeId::Doc(spec_name.to_string(), entry.kind.clone()),
    ];
    // Requirement 2.10: patch the search-match background onto every span's
    // own style, preserving whatever fg/modifiers that span already carries
    // (e.g. the completed-progress bold green below).
    let highlight = |style: Style| {
        if is_search_match(&own_path, search_matches) {
            style.bg(SEARCH_MATCH_BG)
        } else {
            style
        }
    };

    let mut spans = vec![Span::styled(format!("{symbol} {filename}"), highlight(base_style))];

    if entry.kind == DocKind::Tasks {
        if let Some(progress) = entry.progress {
            // Requirement 4.1's "n/m" progress suffix; requirement 4.4's
            // completed-highlight style when done == total.
            let suffix = format!(" {}/{}", progress.done, progress.total);
            let style = if progress.done == progress.total {
                Style::new().add_modifier(Modifier::BOLD).fg(Color::Green)
            } else {
                base_style
            };
            spans.push(Span::styled(suffix, highlight(style)));
        } else if entry.exists {
            // Requirement 4.3: tasks.md exists but has no checkboxes at all.
            spans.push(Span::styled(" 진행률 없음", highlight(base_style)));
        }
    }

    TreeItem::new_leaf(
        NodeId::Doc(spec_name.to_string(), entry.kind.clone()),
        Line::from(spans),
    )
}

/// Build the top-level "Steering" group node and its children (requirement
/// 2.5).
fn steering_group_item(docs: &[SteeringDoc], search_matches: &[Vec<NodeId>]) -> TreeItem<'static, NodeId> {
    let children: Vec<TreeItem<'static, NodeId>> = docs
        .iter()
        .map(|d| steering_item(d, search_matches))
        .collect();
    let highlighted = is_search_match(&[NodeId::SteeringGroup], search_matches);
    TreeItem::new(NodeId::SteeringGroup, labeled_line("Steering".to_string(), highlighted), children)
        .expect("steering doc names are unique by construction")
}

/// Literal front-matter vocabulary for `inclusion` (requirement 2.8) — not
/// re-capitalized or translated.
fn inclusion_str(inclusion: Inclusion) -> &'static str {
    match inclusion {
        Inclusion::Always => "always",
        Inclusion::Manual => "manual",
        Inclusion::FileMatch => "fileMatch",
        Inclusion::Auto => "auto",
    }
}

fn steering_item(doc: &SteeringDoc, search_matches: &[Vec<NodeId>]) -> TreeItem<'static, NodeId> {
    let label = format!("{} [{}]", doc.name, inclusion_str(doc.inclusion));
    let own_path = vec![NodeId::SteeringGroup, NodeId::Steering(doc.name.clone())];
    let highlighted = is_search_match(&own_path, search_matches);
    TreeItem::new_leaf(NodeId::Steering(doc.name.clone()), labeled_line(label, highlighted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/kiro"))
    }

    fn build_root() -> SpecRoot {
        let snapshot = crate::app::load_snapshot(&fixtures_root());
        crate::spec::build(&snapshot)
    }

    fn buffer_text(buffer: &Buffer) -> Vec<String> {
        let area = buffer.area;
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer.get(x, y).symbol())
                    .collect::<String>()
            })
            .collect()
    }

    /// Strip whitespace so a row scraped from the buffer can be compared
    /// against expected text containing CJK characters: ratatui renders
    /// each double-width glyph across two cells (glyph + blank spacer),
    /// which otherwise shows up as stray spaces when cells are joined.
    fn strip_ws(s: &str) -> String {
        s.chars().filter(|c| !c.is_whitespace()).collect()
    }

    /// Row index (0-based) of the first row containing `needle`, or panics.
    fn row_index(rows: &[String], needle: &str) -> usize {
        rows.iter()
            .position(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("expected a row containing {needle:?}, got:\n{rows:?}"))
    }

    /// Locate the first row containing (ASCII) `needle` and return the
    /// `(modifier, fg)` style of the cell at `needle`'s starting column.
    fn cell_style_at(buffer: &Buffer, needle: &str) -> (Modifier, Color) {
        let area = buffer.area;
        for y in 0..area.height {
            let row: String = (0..area.width).map(|x| buffer.get(x, y).symbol()).collect();
            if let Some(idx) = row.find(needle) {
                let cell = buffer.get(idx as u16, y);
                return (cell.modifier, cell.fg);
            }
        }
        panic!("expected a row containing {needle:?}");
    }

    /// Same lookup as [`cell_style_at`], but the background color -- what
    /// task 19.4's search-match highlight (requirement 2.10) actually sets.
    fn cell_bg_at(buffer: &Buffer, needle: &str) -> Color {
        let area = buffer.area;
        for y in 0..area.height {
            let row: String = (0..area.width).map(|x| buffer.get(x, y).symbol()).collect();
            if let Some(idx) = row.find(needle) {
                return buffer.get(idx as u16, y).bg;
            }
        }
        panic!("expected a row containing {needle:?}");
    }

    /// Takes `SpecRoot` by value (`SpecRoot` has no `Clone`, and `render`
    /// now needs a `&TreeSource`, which owns whatever `SpecRoot` it wraps)
    /// -- every call site only uses its `root` once, right before this
    /// call, so moving it in is never a loss.
    fn render_root(
        root: SpecRoot,
        open: Vec<Vec<NodeId>>,
    ) -> Buffer {
        render_root_with_matches(root, open, &[])
    }

    /// Same as [`render_root`], plus a `search_matches` list (task 19.4,
    /// requirement 2.10) for tests exercising the search-match highlight.
    fn render_root_with_matches(
        root: SpecRoot,
        open: Vec<Vec<NodeId>>,
        search_matches: &[Vec<NodeId>],
    ) -> Buffer {
        render_root_full(root, open, None, search_matches)
    }

    /// Full-control variant: also sets the tree's *selection* before
    /// rendering (task 19.4's search reveals a match by both opening its
    /// ancestors *and* selecting it -- this lets a test assert both at
    /// once, matching what `app::search::tree_search` actually does).
    fn render_root_full(
        root: SpecRoot,
        open: Vec<Vec<NodeId>>,
        selected: Option<Vec<NodeId>>,
        search_matches: &[Vec<NodeId>],
    ) -> Buffer {
        let backend = TestBackend::new(100, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let mut tree_state: TreeState<NodeId> = TreeState::default();
        for path in open {
            tree_state.open(path);
        }
        if let Some(selected) = selected {
            tree_state.select(selected);
        }
        let source = TreeSource::Kiro(root);
        terminal
            .draw(|frame| {
                let area = frame.area();
                render(frame, area, &source, &mut tree_state, SortKey::Name, search_matches, &mut None);
            })
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    // --- task 19.2: tree title shows the current sort key (requirement 2.9)

    #[test]
    fn tree_title_shows_the_current_sort_key() {
        let root = TreeSource::Kiro(build_root());
        for (key, label) in [
            (SortKey::Name, "이름"),
            (SortKey::Phase, "단계"),
            (SortKey::Updated, "최근 갱신"),
            (SortKey::Progress, "진행률"),
        ] {
            let backend = TestBackend::new(100, 40);
            let mut terminal = Terminal::new(backend).expect("terminal");
            let mut tree_state: TreeState<NodeId> = TreeState::default();
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    render(frame, area, &root, &mut tree_state, key, &[], &mut None);
                })
                .expect("draw");
            let rows = buffer_text(terminal.backend().buffer());
            // CJK labels render one glyph per two cells (glyph + blank
            // spacer) -- compare with whitespace stripped, same as this
            // module's other CJK-label tests (`strip_ws`).
            let needle = strip_ws(&format!("Specs[{label}]"));
            assert!(
                rows.iter().any(|row| strip_ws(row).contains(&needle)),
                "expected the tree title to show sort key {key:?} ({label:?}), got:\n{rows:?}"
            );
        }
    }

    #[test]
    fn spec_with_ok_meta_shows_phase_badge() {
        let root = build_root();
        let buffer = render_root(root, vec![]);
        let rows = buffer_text(&buffer);

        // sample-signup/spec.json has "phase": "implementation".
        assert!(rows.iter().any(|row| row.contains("sample-signup [implementation]")));
    }

    #[test]
    fn spec_with_err_meta_shows_warning_badge_not_phase() {
        let root = build_root();
        let buffer = render_root(root, vec![]);
        let rows = buffer_text(&buffer);

        let idx = row_index(&rows, "broken-json !");
        assert!(!rows[idx].contains('['), "broken-json row should show no phase badge");
    }

    #[test]
    fn expanding_spec_reveals_docs_in_canonical_order_with_status_symbols() {
        let root = build_root();
        let buffer = render_root(root, vec![vec![NodeId::Spec("sample-signup".to_string())]]);
        let rows = buffer_text(&buffer);

        // sample-signup/spec.json approvals: requirements approved,
        // bizProcess generated-not-approved, design generated-not-approved,
        // no "tasks" key at all (=> NoRecord).
        let spec_row = row_index(&rows, "sample-signup [implementation]");
        let req_row = row_index(&rows, "● requirements.md");
        let biz_row = row_index(&rows, "○ biz-process.md");
        let design_row = row_index(&rows, "○ design.md");
        let tasks_row = row_index(&rows, "? tasks.md");

        assert!(spec_row < req_row);
        assert!(req_row < biz_row);
        assert!(biz_row < design_row);
        assert!(design_row < tasks_row);
    }

    #[test]
    fn missing_doc_row_is_styled_differently_from_present_doc_row() {
        let root = build_root();
        let buffer = render_root(root, vec![vec![NodeId::Spec("no-approvals".to_string())]]);
        let rows = buffer_text(&buffer);

        // no-approvals: requirements.md exists; design.md/biz-process.md/
        // tasks.md do not (known from spec::build's own tests).
        assert!(rows.iter().any(|row| row.contains("requirements.md")));
        assert!(rows.iter().any(|row| row.contains("design.md")));

        let (present_mod, _) = cell_style_at(&buffer, "requirements.md");
        let (missing_mod, _) = cell_style_at(&buffer, "design.md");

        assert_ne!(present_mod, missing_mod);
        assert!(missing_mod.contains(Modifier::DIM));
        assert!(!present_mod.contains(Modifier::DIM));
    }

    #[test]
    fn tasks_progress_shown_and_no_checkboxes_message_shown() {
        let root = build_root();
        let buffer = render_root(
            root,
            vec![
                vec![NodeId::Spec("sample-signup".to_string())],
                vec![NodeId::Spec("no-checkboxes".to_string())],
            ],
        );
        let rows = buffer_text(&buffer);

        // sample-signup/tasks.md hand-counted: 3 done / 5 total (same fixture
        // task 3.3 used).
        assert!(rows.iter().any(|row| row.contains("tasks.md 3/5")));

        // no-checkboxes/tasks.md has no checkbox lines at all. CJK glyphs
        // render as double-width cells with a blank spacer cell trailing
        // each one, so compare with whitespace stripped from both sides
        // (same pattern `markdown::mod`'s own CJK tests use) rather than
        // requiring an exact byte-for-byte substring match.
        let no_ws_needle = strip_ws("tasks.md 진행률 없음");
        assert!(
            rows.iter().any(|row| strip_ws(row).contains(&no_ws_needle)),
            "expected a 진행률 없음 row, got:\n{rows:?}"
        );
    }

    #[test]
    fn all_done_tasks_progress_is_bold_and_green() {
        // None of the on-disk fixtures happen to have an all-checkboxes-done
        // tasks.md, so this constructs a minimal `SpecRoot` in memory
        // (`Spec`/`DocEntry`/`Progress` are all public structs — same
        // pattern `spec::build`'s own tests use) to exercise requirement
        // 4.4's "완료 강조 스타일" directly, alongside a non-complete
        // sibling to prove the styles actually differ.
        use crate::spec::{DocStatus, Progress, Spec, SpecRoot};
        use std::path::PathBuf;

        let spec = Spec {
            name: "all-done".to_string(),
            dir: PathBuf::from("/does/not/matter"),
            meta: Err(crate::spec::MetaError::InvalidJson("n/a".to_string())),
            docs: vec![DocEntry {
                kind: DocKind::Tasks,
                path: PathBuf::from("/does/not/matter/tasks.md"),
                exists: true,
                status: DocStatus::NotTracked,
                progress: Some(Progress { done: 5, total: 5 }),
            }],
            definition: None,
        };
        let root = SpecRoot {
            specs: vec![spec],
            steering: Vec::new(),
        };

        let buffer = render_root(root, vec![vec![NodeId::Spec("all-done".to_string())]]);
        let rows = buffer_text(&buffer);
        assert!(rows.iter().any(|row| row.contains("tasks.md 5/5")));

        let (modifier, fg) = cell_style_at(&buffer, "5/5");
        assert!(modifier.contains(Modifier::BOLD));
        assert_eq!(fg, Color::Green);
    }

    #[test]
    fn steering_group_shows_inclusion_badges() {
        let root = build_root();
        let buffer = render_root(root, vec![vec![NodeId::SteeringGroup]]);
        let rows = buffer_text(&buffer);

        assert!(rows.iter().any(|row| row.contains("product.md [always]")));
        assert!(rows.iter().any(|row| row.contains("tech.md [always]")));
        assert!(rows.iter().any(|row| row.contains("domain-terms.md [manual]")));

        // Steering rows are a group separate from any spec's rows: none of
        // them should collide with a spec row's phase-badge syntax.
        let steering_row = row_index(&rows, "Steering");
        let product_row = row_index(&rows, "product.md [always]");
        assert!(steering_row < product_row);
    }

    #[test]
    fn other_doc_kind_shows_bare_filename_with_no_status_symbol() {
        let root = build_root();
        let buffer = render_root(root, vec![vec![NodeId::Spec("with-extra".to_string())]]);
        let rows = buffer_text(&buffer);

        let notes_row = rows
            .iter()
            .find(|row| row.contains("notes.md"))
            .expect("notes.md row present");

        for symbol in ["·", "○", "●", "?"] {
            assert!(
                !notes_row.contains(symbol),
                "notes.md row should carry no status symbol, got: {notes_row:?}"
            );
        }
    }

    // --- task 19.4: tree panel search-match highlight (requirement 2.10)

    #[test]
    fn matched_spec_row_gets_the_search_highlight_background() {
        let root = build_root();
        let matches = vec![vec![NodeId::Spec("no-approvals".to_string())]];
        let buffer = render_root_with_matches(root, vec![], &matches);
        let rows = buffer_text(&buffer);

        assert!(rows.iter().any(|row| row.contains("no-approvals")));
        assert_eq!(cell_bg_at(&buffer, "no-approvals"), Color::Yellow);
    }

    #[test]
    fn unmatched_rows_carry_no_search_highlight() {
        let root = build_root();
        let matches = vec![vec![NodeId::Spec("no-approvals".to_string())]];
        let buffer = render_root_with_matches(root, vec![], &matches);

        assert_ne!(cell_bg_at(&buffer, "sample-signup"), Color::Yellow);
    }

    #[test]
    fn matched_doc_row_gets_highlighted_and_current_match_is_selected() {
        let root = build_root();
        let matched_path = vec![
            NodeId::Spec("sample-signup".to_string()),
            NodeId::Doc("sample-signup".to_string(), DocKind::Requirements),
        ];
        let matches = vec![matched_path.clone()];
        // Mirrors what `app::search::tree_search` does on a match: open the
        // spec ancestor and select the doc itself.
        let buffer = render_root_full(
            root,
            vec![vec![NodeId::Spec("sample-signup".to_string())]],
            Some(matched_path),
            &matches,
        );
        let rows = buffer_text(&buffer);

        let doc_row = row_index(&rows, "requirements.md");
        assert_eq!(cell_bg_at(&buffer, "requirements.md"), Color::Yellow);
        // The Tree widget's own selection-highlight (`REVERSED`) lands on
        // the same row -- requirement 2.10's "선택 이동·강조" together.
        let (modifier, _) = cell_style_at(&buffer, "requirements.md");
        assert!(
            modifier.contains(Modifier::REVERSED),
            "expected the matched+selected row (row {doc_row}) to carry REVERSED, got {modifier:?}"
        );
    }

    #[test]
    fn matched_steering_doc_row_gets_the_search_highlight_background() {
        let root = build_root();
        let matches = vec![vec![NodeId::SteeringGroup, NodeId::Steering("product.md".to_string())]];
        let buffer = render_root_with_matches(root, vec![vec![NodeId::SteeringGroup]], &matches);

        assert_eq!(cell_bg_at(&buffer, "product.md"), Color::Yellow);
    }

    #[test]
    fn no_search_matches_leaves_every_row_unhighlighted() {
        let root = build_root();
        let buffer = render_root_with_matches(root, vec![], &[]);

        assert_ne!(cell_bg_at(&buffer, "no-approvals"), Color::Yellow);
    }

    // Task 12.1 foundation: `app::mouse::tree_identifier_at` must resolve a
    // real on-screen coordinate to the `NodeId` the tree widget actually
    // rendered there -- kept here (rather than in `app::mouse`'s own test
    // module) because it needs a real `render` pass, and `app` must not
    // depend on `ui` (design.md's one-directional layering).
    #[test]
    fn tree_identifier_at_resolves_the_clicked_row() {
        let root = TreeSource::Kiro(build_root());
        let backend = TestBackend::new(100, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let mut tree_state: TreeState<NodeId> = TreeState::default();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render(frame, area, &root, &mut tree_state, SortKey::Name, &[], &mut None);
            })
            .expect("draw");

        let rows = buffer_text(terminal.backend().buffer());
        let spec_row = row_index(&rows, "sample-signup [implementation]") as u16;

        let identifier = crate::app::mouse::tree_identifier_at(&tree_state, 2, spec_row);
        assert_eq!(
            identifier,
            Some(&[NodeId::Spec("sample-signup".to_string())][..])
        );

        // Row 39 is the bottom border row of a 40-row area -- outside the
        // tree's inner `last_area` entirely, so it must resolve to nothing
        // rather than the last item rendered.
        let outside = crate::app::mouse::tree_identifier_at(&tree_state, 2, 39);
        assert_eq!(outside, None);
    }

    // --- spec-viewer-files-tree-scroll-latency ---------------------------

    fn flat_files_tree(n: usize) -> FsTree {
        let root = std::path::PathBuf::from("/synthetic");
        let mut entries: Vec<FsEntry> = (0..n)
            .map(|i| FsEntry {
                path: root.join(format!("f{i:06}.md")),
                is_dir: false,
                depth: 1,
            })
            .collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        FsTree { root, entries }
    }

    fn render_files_source(
        tree: &FsTree,
        tree_state: &mut TreeState<NodeId>,
        search_matches: &[Vec<NodeId>],
        cache: &mut Option<crate::app::FilesTreeItemCache>,
    ) -> Buffer {
        let backend = TestBackend::new(100, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let source = TreeSource::Files(tree.clone());
        terminal
            .draw(|frame| {
                let area = frame.area();
                render(frame, area, &source, tree_state, SortKey::Name, search_matches, cache);
            })
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    /// 1.1/1.2 -> 2.1: a repeated render of an unchanged large `Files` tree
    /// must reuse the cached `Vec<TreeItem>` build instead of reallocating
    /// it every call -- the *second* render (cache hit) must be
    /// substantially cheaper than the *first* (cache miss, has to build).
    /// A ratio assertion (not an absolute-ms budget) stays robust across
    /// machine speeds, the same pattern used by
    /// `spec-viewer-watch-startup-latency`'s regression test.
    #[test]
    fn files_tree_render_reuses_cached_items_when_unchanged() {
        let tree = flat_files_tree(20_000);
        let mut tree_state: TreeState<NodeId> = TreeState::default();
        let mut cache: Option<crate::app::FilesTreeItemCache> = None;

        let t0 = std::time::Instant::now();
        render_files_source(&tree, &mut tree_state, &[], &mut cache);
        let first = t0.elapsed();

        let t1 = std::time::Instant::now();
        let buf = render_files_source(&tree, &mut tree_state, &[], &mut cache);
        let second = t1.elapsed();

        assert!(cache.is_some(), "expected the cache to be populated after the first render");
        // Still renders correctly from the cache, not a blank/stale panel.
        let rows = buffer_text(&buf);
        assert!(rows.iter().any(|r| r.contains("f000000.md")), "expected a real entry visible, got:\n{rows:?}");

        assert!(
            second < first / 2,
            "expected the cache-hit render ({second:?}) to be well under half the cache-miss \
             render ({first:?}) -- regression: the Vec<TreeItem> build is not being reused"
        );
    }

    /// 2.2/3.2: when the active tree search's matches change, the cache
    /// must not serve stale highlighting -- the rebuilt items reflect the
    /// new `search_matches` on the very next render.
    #[test]
    fn files_tree_render_rebuilds_when_search_matches_change() {
        let tree = flat_files_tree(50);
        let mut tree_state: TreeState<NodeId> = TreeState::default();
        let mut cache: Option<crate::app::FilesTreeItemCache> = None;

        render_files_source(&tree, &mut tree_state, &[], &mut cache);

        let target = vec![NodeId::File(tree.root.join("f000010.md"))];
        let buf = render_files_source(&tree, &mut tree_state, std::slice::from_ref(&target), &mut cache);

        let bg = cell_bg_at(&buf, "f000010.md");
        assert_eq!(bg, SEARCH_MATCH_BG, "expected the newly-searched entry to be highlighted, not a stale cached (unhighlighted) render");
    }

    /// 2.2/3.1: when the underlying entries change (e.g. a rescan after a
    /// file was added), the cache must not serve the old file list.
    #[test]
    fn files_tree_render_rebuilds_when_entries_change() {
        let mut tree_state: TreeState<NodeId> = TreeState::default();
        let mut cache: Option<crate::app::FilesTreeItemCache> = None;

        let before = flat_files_tree(5);
        render_files_source(&before, &mut tree_state, &[], &mut cache);

        let after = flat_files_tree(6); // one more entry, as a rescan would add
        let buf = render_files_source(&after, &mut tree_state, &[], &mut cache);

        let rows = buffer_text(&buf);
        assert!(
            rows.iter().any(|r| r.contains("f000005.md")),
            "expected the newly-added entry to appear after entries changed, got:\n{rows:?}"
        );
    }
}
