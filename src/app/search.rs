//! Plain-text search over the currently displayed document (design.md File
//! Structure Plan: "search.rs # Plain-text search logic"; requirements
//! 6.5-6.9).
//!
//! This module owns only the search *engine* — matching against
//! `Rendered.plain`, cycling through matches, and clearing search state. It
//! is called directly (by tests here, and eventually by task 5.6's
//! `/`-key-driven search-input popup, which is out of this task's scope) —
//! it does not itself bind any keys or drive any popup.

use super::{rendered_of, AppState, SearchState};
use crate::spec::{FsEntry, FsTree, NodeId, SpecRoot, TreeSource};

/// Run a plain-text, case-insensitive search for `query` over the current
/// document's `Rendered.plain` lines (requirements 6.5, 6.8).
///
/// Sets `state.search.query` unconditionally. If `query` is empty or the
/// current `state.doc` carries no `Rendered` payload, clears matches/current
/// (nothing to search). Otherwise collects every line index whose
/// lowercased text contains the lowercased `query`, in ascending order. On a
/// non-empty result, jumps to the first match (6.5); on no matches, leaves
/// `state.scroll` untouched — the empty `matches` alongside a non-empty
/// `query` is the "일치 없음" signal (6.7) for a later UI task to render.
pub fn search(state: &mut AppState, query: &str) {
    state.search.query = query.to_string();

    if query.is_empty() {
        state.search.matches = Vec::new();
        state.search.current = None;
        return;
    }

    let Some(rendered) = rendered_of(&state.doc) else {
        state.search.matches = Vec::new();
        state.search.current = None;
        return;
    };

    let needle = query.to_lowercase();
    let matches: Vec<usize> = rendered
        .plain
        .iter()
        .enumerate()
        .filter(|(_, line)| line.to_lowercase().contains(&needle))
        .map(|(idx, _)| idx)
        .collect();

    if matches.is_empty() {
        state.search.matches = matches;
        state.search.current = None;
    } else {
        state.scroll = matches[0];
        state.search.matches = matches;
        state.search.current = Some(0);
    }
}

/// Advance to the next match, cyclically (requirement 6.6). No-op when
/// there are no matches.
pub fn next_match(state: &mut AppState) {
    let len = state.search.matches.len();
    if len == 0 {
        return;
    }
    let current = state.search.current.unwrap_or(0);
    let new_current = (current + 1) % len;
    state.search.current = Some(new_current);
    state.scroll = state.search.matches[new_current];
}

/// Retreat to the previous match, cyclically (requirement 6.6). No-op when
/// there are no matches.
pub fn prev_match(state: &mut AppState) {
    let len = state.search.matches.len();
    if len == 0 {
        return;
    }
    let current = state.search.current.unwrap_or(0);
    let new_current = (current + len - 1) % len;
    state.search.current = Some(new_current);
    state.scroll = state.search.matches[new_current];
}

/// Reset search state to its default, e.g. when the displayed document
/// changes (requirement 6.9). Deliberately leaves `state.scroll` alone —
/// call sites decide whether scroll should also reset.
pub fn clear_search(state: &mut AppState) {
    state.search = SearchState::default();
}

// --- Tree panel search (requirement 2.10) -----------------------------
//
// A separate engine from the plain-text search above — it matches tree
// *node names*, not document lines, and "jumping" to a match means
// expanding the match's collapsed ancestors and moving the tree selection
// to it, rather than scrolling. Kept in this module (rather than a new
// file) since it is the same kind of engine, over a different haystack.

/// One flattened tree row: the full identifier path `tui_tree_widget` needs
/// to select/open it, and the display name requirement 2.10's "노드 이름
/// 부분 일치" matches against.
type TreeRow = (Vec<NodeId>, String);

/// Flatten `root` into every node's `(path, name)`, in the same order
/// `ui::tree_panel::render` builds its `TreeItem`s in — not just the
/// currently visible/expanded ones, since a collapsed match still needs to
/// be found (2.10's "접힌 폴더 안의 일치는 조상을 펼쳐서 이동").
fn flatten_tree(root: &TreeSource) -> Vec<TreeRow> {
    match root {
        TreeSource::Kiro(root) => flatten_kiro(root),
        TreeSource::Files(tree) => flatten_files(tree),
    }
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn flatten_kiro(root: &SpecRoot) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    for spec in &root.specs {
        let spec_path = vec![NodeId::Spec(spec.name.clone())];
        rows.push((spec_path.clone(), spec.name.clone()));
        for doc in &spec.docs {
            let mut path = spec_path.clone();
            path.push(NodeId::Doc(spec.name.clone(), doc.kind.clone()));
            rows.push((path, file_name(&doc.path)));
        }
    }

    // Steering group placed last, matching `tree_panel::render_kiro`.
    let steering_path = vec![NodeId::SteeringGroup];
    rows.push((steering_path.clone(), "Steering".to_string()));
    for doc in &root.steering {
        let mut path = steering_path.clone();
        path.push(NodeId::Steering(doc.name.clone()));
        rows.push((path, doc.name.clone()));
    }

    rows
}

/// Mirrors `ui::tree_panel::build_files_items`'s reconstruction of nested
/// paths from `FsTree`'s flat, depth-tagged, path-sorted entries, but only
/// needs each entry's own `(path, name)` — no actual `TreeItem` tree.
fn flatten_files(tree: &FsTree) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    let mut ancestors: Vec<NodeId> = Vec::new();
    let mut depths: Vec<u8> = Vec::new();

    for entry in &tree.entries {
        while depths.last().is_some_and(|&d| d >= entry.depth) {
            ancestors.pop();
            depths.pop();
        }

        let id = entry_node_id(entry);
        let mut path = ancestors.clone();
        path.push(id.clone());
        rows.push((path, file_name(&entry.path)));

        if entry.is_dir {
            ancestors.push(id);
            depths.push(entry.depth);
        }
    }

    rows
}

fn entry_node_id(entry: &FsEntry) -> NodeId {
    if entry.is_dir {
        NodeId::Dir(entry.path.clone())
    } else {
        NodeId::File(entry.path.clone())
    }
}

/// Reveal and select a matched node: opens every proper-prefix ancestor of
/// `path` (2.10 "접힌 폴더 안의 일치는 조상을 펼쳐서 이동") and moves the tree
/// selection onto it (2.10 "선택 이동" — the actual highlight is
/// `ui::tree_panel`'s own selection-highlight style, reused as-is).
fn reveal_and_select(state: &mut AppState, path: &[NodeId]) {
    for depth in 1..path.len() {
        state.tree.open(path[..depth].to_vec());
    }
    state.tree.select(path.to_vec());
}

/// Run a case-insensitive substring search for `query` over the tree's node
/// names (requirements 2.10, 6.8). Same shape as [`search`] above: sets
/// `state.tree_search.query` unconditionally, and on a non-empty result
/// jumps to (and selects/reveals) the first match. `state.tree_search.matches`
/// only ever holds `0..state.tree_matches.len()` — a placeholder the status
/// bar's "(i/n)" count reads via the shared `SearchState`/`status_bar`
/// rendering path; the real matched paths live in `state.tree_matches`.
pub fn tree_search(state: &mut AppState, query: &str) {
    state.tree_search.query = query.to_string();

    if query.is_empty() {
        state.tree_search.matches = Vec::new();
        state.tree_search.current = None;
        state.tree_matches = Vec::new();
        return;
    }

    let needle = query.to_lowercase();
    let paths: Vec<Vec<NodeId>> = flatten_tree(&state.root)
        .into_iter()
        .filter(|(_, name)| name.to_lowercase().contains(&needle))
        .map(|(path, _)| path)
        .collect();

    if paths.is_empty() {
        state.tree_search.matches = Vec::new();
        state.tree_search.current = None;
    } else {
        reveal_and_select(state, &paths[0]);
        state.tree_search.matches = (0..paths.len()).collect();
        state.tree_search.current = Some(0);
    }
    state.tree_matches = paths;
}

/// Advance to the next tree match, cyclically (requirement 2.10's "이전/다음
/// 키로 일치 순환"). No-op when there are no matches.
pub fn tree_next_match(state: &mut AppState) {
    let len = state.tree_matches.len();
    if len == 0 {
        return;
    }
    let current = state.tree_search.current.unwrap_or(0);
    let new_current = (current + 1) % len;
    state.tree_search.current = Some(new_current);
    let path = state.tree_matches[new_current].clone();
    reveal_and_select(state, &path);
}

/// Retreat to the previous tree match, cyclically. No-op when there are no
/// matches.
pub fn tree_prev_match(state: &mut AppState) {
    let len = state.tree_matches.len();
    if len == 0 {
        return;
    }
    let current = state.tree_search.current.unwrap_or(0);
    let new_current = (current + len - 1) % len;
    state.tree_search.current = Some(new_current);
    let path = state.tree_matches[new_current].clone();
    reveal_and_select(state, &path);
}

/// Reset tree search state to its default (requirement 2.10 "Esc 로 해제").
/// Deliberately leaves `state.tree`'s selection/opened set alone — clearing
/// the search should not also collapse whatever it revealed or move the
/// selection away from the match the user just landed on.
pub fn clear_tree_search(state: &mut AppState) {
    state.tree_search = SearchState::default();
    state.tree_matches = Vec::new();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{DocView, FileInfo, Panel, TreeMode, WatchStatus};
    use crate::markdown;
    use crate::spec::{self, TreeSource};
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/kiro"))
    }

    fn test_state() -> AppState {
        let snapshot = super::super::loader::load_snapshot(&fixtures_root());
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

    fn load_inline_doc(state: &mut AppState, src: &str) {
        state.doc = DocView::Rendered {
            path: PathBuf::from("inline.md"),
            r: markdown::render(src, 80),
            meta: FileInfo::default(),
        };
        state.scroll = 0;
    }

    // Blank lines separate each into its own paragraph/plain-text line —
    // pulldown-cmark merges consecutive non-blank lines into a single
    // wrapped paragraph, which would collapse these into one `plain` entry
    // and defeat the point of this fixture (exact control over which line
    // index matches).
    const FIXTURE: &str = "line one\n\nLINE TWO\n\nline three\n\nfour";

    #[test]
    fn case_insensitive_match_jumps_to_first_match() {
        let mut state = test_state();
        load_inline_doc(&mut state, FIXTURE);

        search(&mut state, "line two");

        assert_eq!(state.search.query, "line two");
        assert_eq!(state.search.matches, vec![1]);
        assert_eq!(state.search.current, Some(0));
        assert_eq!(state.scroll, 1);
    }

    #[test]
    fn query_matching_multiple_lines_collects_all_in_order() {
        let mut state = test_state();
        load_inline_doc(&mut state, FIXTURE);

        search(&mut state, "line");

        assert_eq!(state.search.matches, vec![0, 1, 2]);
        assert_eq!(state.search.current, Some(0));
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn query_matching_nothing_leaves_scroll_unchanged() {
        let mut state = test_state();
        load_inline_doc(&mut state, FIXTURE);
        state.scroll = 2;

        search(&mut state, "nonexistent");

        assert!(state.search.matches.is_empty());
        assert_eq!(state.search.current, None);
        assert_eq!(state.scroll, 2);
    }

    #[test]
    fn next_and_prev_cycle_through_matches_with_wraparound() {
        let mut state = test_state();
        load_inline_doc(&mut state, FIXTURE);
        search(&mut state, "line"); // matches [0, 1, 2], current 0

        next_match(&mut state);
        assert_eq!(state.search.current, Some(1));
        assert_eq!(state.scroll, 1);

        next_match(&mut state);
        assert_eq!(state.search.current, Some(2));
        assert_eq!(state.scroll, 2);

        // Wrap past the last match back to the first.
        next_match(&mut state);
        assert_eq!(state.search.current, Some(0));
        assert_eq!(state.scroll, 0);

        // Wrap backward past the first match back to the last.
        prev_match(&mut state);
        assert_eq!(state.search.current, Some(2));
        assert_eq!(state.scroll, 2);

        prev_match(&mut state);
        assert_eq!(state.search.current, Some(1));
        assert_eq!(state.scroll, 1);
    }

    #[test]
    fn next_and_prev_on_empty_matches_are_no_ops() {
        let mut state = test_state();
        load_inline_doc(&mut state, FIXTURE);
        // No search run yet — matches is empty by default.

        next_match(&mut state);
        assert_eq!(state.search.current, None);
        assert_eq!(state.scroll, 0);

        prev_match(&mut state);
        assert_eq!(state.search.current, None);
        assert_eq!(state.scroll, 0);

        // A failed search also leaves matches empty.
        search(&mut state, "nonexistent");
        next_match(&mut state);
        assert_eq!(state.search.current, None);
        prev_match(&mut state);
        assert_eq!(state.search.current, None);
    }

    #[test]
    fn searching_a_doc_without_rendered_payload_is_a_no_op_result() {
        let mut state = test_state();
        state.doc = DocView::Empty;

        search(&mut state, "anything");

        assert!(state.search.matches.is_empty());
        assert_eq!(state.search.current, None);

        state.doc = DocView::Missing(PathBuf::from("missing.md"));
        search(&mut state, "anything");
        assert!(state.search.matches.is_empty());
        assert_eq!(state.search.current, None);
    }

    #[test]
    fn clear_search_resets_state_but_not_scroll() {
        let mut state = test_state();
        load_inline_doc(&mut state, FIXTURE);
        search(&mut state, "line");
        state.scroll = 2;

        clear_search(&mut state);

        assert_eq!(state.search, SearchState::default());
        assert_eq!(state.scroll, 2);
        // Focus untouched by this reset (sanity check it only touches search).
        assert_eq!(state.focus, Panel::Tree);
    }

    // --- task 19.4: tree panel search (requirement 2.10) -----------------

    #[test]
    fn tree_search_case_insensitive_single_match_selects_the_spec() {
        let mut state = test_state();

        tree_search(&mut state, "NO-APPROVALS");

        assert_eq!(state.tree_search.query, "NO-APPROVALS");
        assert_eq!(state.tree_search.matches, vec![0]);
        assert_eq!(state.tree_search.current, Some(0));
        assert_eq!(
            state.tree_matches,
            vec![vec![NodeId::Spec("no-approvals".to_string())]]
        );
        assert_eq!(state.tree.selected(), [NodeId::Spec("no-approvals".to_string())]);
    }

    #[test]
    fn tree_search_expands_collapsed_ancestor_to_reach_a_doc_match() {
        let mut state = test_state();
        // Sanity: nothing is expanded yet, so the match starts collapsed.
        assert!(state.tree.opened().is_empty());

        tree_search(&mut state, "requirements.md");

        // Every spec with a requirements.md doc matches -- at least one, and
        // the first match's spec ancestor must now be open.
        assert!(!state.tree_matches.is_empty());
        let first = &state.tree_matches[0];
        assert_eq!(first.len(), 2);
        let NodeId::Spec(spec_name) = &first[0] else {
            panic!("expected a Spec ancestor, got {:?}", first[0]);
        };
        assert!(state.tree.opened().contains(&vec![NodeId::Spec(spec_name.clone())]));
        assert_eq!(state.tree.selected(), first.as_slice());
    }

    #[test]
    fn tree_search_next_and_prev_cycle_through_matches_with_wraparound() {
        let mut state = test_state();
        tree_search(&mut state, "requirements.md");
        let total = state.tree_matches.len();
        assert!(total > 1, "fixture should have multiple requirements.md docs");

        tree_next_match(&mut state);
        assert_eq!(state.tree_search.current, Some(1));
        assert_eq!(state.tree.selected(), state.tree_matches[1].as_slice());

        // Wrap forward past the last match back to the first.
        for _ in 0..(total - 1) {
            tree_next_match(&mut state);
        }
        assert_eq!(state.tree_search.current, Some(0));
        assert_eq!(state.tree.selected(), state.tree_matches[0].as_slice());

        // Wrap backward past the first match back to the last.
        tree_prev_match(&mut state);
        assert_eq!(state.tree_search.current, Some(total - 1));
        assert_eq!(state.tree.selected(), state.tree_matches[total - 1].as_slice());
    }

    #[test]
    fn tree_search_next_and_prev_on_empty_matches_are_no_ops() {
        let mut state = test_state();
        state.tree.select(vec![NodeId::Spec("sample-signup".to_string())]);

        tree_next_match(&mut state);
        assert_eq!(state.tree.selected(), [NodeId::Spec("sample-signup".to_string())]);

        tree_search(&mut state, "zzz-nonexistent");
        assert!(state.tree_search.matches.is_empty());
        assert_eq!(state.tree_search.current, None);
        // No-match search leaves the prior selection untouched.
        assert_eq!(state.tree.selected(), [NodeId::Spec("sample-signup".to_string())]);

        tree_prev_match(&mut state);
        assert_eq!(state.tree.selected(), [NodeId::Spec("sample-signup".to_string())]);
    }

    #[test]
    fn tree_search_matches_steering_doc_names_and_expands_the_group() {
        let mut state = test_state();

        tree_search(&mut state, "product");

        assert_eq!(
            state.tree_matches,
            vec![vec![NodeId::SteeringGroup, NodeId::Steering("product.md".to_string())]]
        );
        assert!(state.tree.opened().contains(&vec![NodeId::SteeringGroup]));
        assert_eq!(
            state.tree.selected(),
            [NodeId::SteeringGroup, NodeId::Steering("product.md".to_string())]
        );
    }

    #[test]
    fn clear_tree_search_resets_state_but_not_selection() {
        let mut state = test_state();
        tree_search(&mut state, "no-approvals");
        assert!(!state.tree_matches.is_empty());

        clear_tree_search(&mut state);

        assert_eq!(state.tree_search, SearchState::default());
        assert!(state.tree_matches.is_empty());
        // The selection/expansion the search left behind survives Esc.
        assert_eq!(state.tree.selected(), [NodeId::Spec("no-approvals".to_string())]);
    }
}
