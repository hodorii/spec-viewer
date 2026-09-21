//! Top-level layout composition (design.md "ui — Panels"; requirements 1.4,
//! 2.7, 8.5). Wires the four already-committed panel renderers
//! (`tree_panel`, `doc_panel`, `status_bar`, `popup`) into one frame: a
//! two-panel tree/doc split above a one-row status bar when the frame is
//! wide enough, collapsing to whichever panel currently has focus when it
//! is not, plus a focus marker and a popup overlay drawn on top.

pub mod tree_panel;
pub mod doc_panel;
pub mod status_bar;
pub mod popup;

use crate::app::{self, AppState, Panel, PanelLayout, Selection, TreeMode};
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::Frame;

/// `NARROW_WIDTH_THRESHOLD`/`TREE_PANEL_PERCENT` themselves live in `app`
/// (`app::doc_panel_width` needs the identical values to predict this
/// module's own split without duplicating -- and risking drifting from --
/// the formula below; tasks.md 20.1). `DOC_PANEL_PERCENT` is only ever used
/// by this module's own tests.
#[cfg(test)]
const DOC_PANEL_PERCENT: u16 = 100 - app::TREE_PANEL_PERCENT;

/// Render the whole application into `frame` (design.md "ui — Panels";
/// requirements 1.4 "병렬 표시", 2.7 "활성 패널 시각적 구분", 8.5 "극소 폭").
pub fn render(frame: &mut Frame, state: &mut AppState) {
    let area = frame.area();

    // Reserve exactly one row at the bottom for the status bar, regardless
    // of width mode -- it is always shown (design.md status_bar spec).
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);
    let main_area = vertical[0];
    let status_area = vertical[1];

    // The area actually occupied by the currently-focused panel this
    // render, used below to place the focus marker (requirement 2.7). In
    // single-panel mode the focused panel *is* `main_area` -- see the
    // focus-marker comment below for why that is still meaningful.
    let focused_area = if !state.tree_visible {
        // Requirements 1.6/1.7: tree hidden entirely (file view, `--tree
        // hidden`, or toggled off) -- the doc panel always fills the whole
        // main area, regardless of `focus` (there is no tree to focus).
        doc_panel::render(frame, main_area, &state.doc, state.scroll, &state.search);
        state.layout = PanelLayout {
            tree: Rect::default(),
            sep: Rect::default(),
            doc: main_area,
        };
        main_area
    } else if state.tree_mode == TreeMode::Single {
        // Requirement 1.10: Single mode always shows exactly one panel at
        // full main-area width, regardless of terminal width -- which one
        // is `state.doc_focus`'s job to say, not the width-based fallback
        // just below (that fallback is Auto's own "좁으면 단일 모드처럼
        // 동작" behavior, keyed off `focus` instead).
        if state.doc_focus {
            doc_panel::render(frame, main_area, &state.doc, state.scroll, &state.search);
            state.layout = PanelLayout {
                tree: Rect::default(),
                sep: Rect::default(),
                doc: main_area,
            };
        } else {
            tree_panel::render(frame, main_area, &state.root, &mut state.tree, state.sort_key, &state.tree_matches, &mut state.files_tree_cache);
            state.layout = PanelLayout {
                tree: main_area,
                sep: Rect::default(),
                doc: Rect::default(),
            };
        }
        main_area
    } else if main_area.width < app::NARROW_WIDTH_THRESHOLD {
        // Single-panel mode (requirement 8.5): only the focused panel
        // renders, using the entire main area. The panel that is *not*
        // showing gets a zero-size rect (design.md "Mouse hit-test from
        // stored layout": a hidden panel's Rect never hit-tests true).
        match state.focus {
            Panel::Tree => {
                tree_panel::render(frame, main_area, &state.root, &mut state.tree, state.sort_key, &state.tree_matches, &mut state.files_tree_cache);
                state.layout = PanelLayout {
                    tree: main_area,
                    sep: Rect::default(),
                    doc: Rect::default(),
                };
            }
            Panel::Doc => {
                doc_panel::render(frame, main_area, &state.doc, state.scroll, &state.search);
                state.layout = PanelLayout {
                    tree: Rect::default(),
                    sep: Rect::default(),
                    doc: main_area,
                };
            }
        }
        main_area
    } else {
        // Two-panel mode: tree/doc split side by side (requirement 1.4
        // "병렬 표시"), with a 1-column drag handle between them
        // (requirement 9.6; design.md ui "사이에 1열 sep") -- `state.split`
        // is the tree's column width, defaulted here on first use and left
        // alone afterwards so a future drag (task 12.4) can adjust it.
        if state.split == 0 {
            state.split = ((main_area.width as u32 * app::TREE_PANEL_PERCENT as u32) / 100) as u16;
        }
        // Defensive clamp only (task 12.4 owns the real "min 20 cols"
        // drag-drag limit): never let a stale/oversized split produce a
        // negative-width doc column after e.g. a terminal shrink.
        let tree_width = state.split.min(main_area.width.saturating_sub(1));

        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(tree_width),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(main_area);
        let tree_area = horizontal[0];
        let sep_area = horizontal[1];
        let doc_area = horizontal[2];

        tree_panel::render(frame, tree_area, &state.root, &mut state.tree, state.sort_key, &state.tree_matches, &mut state.files_tree_cache);
        doc_panel::render(frame, doc_area, &state.doc, state.scroll, &state.search);

        state.layout = PanelLayout {
            tree: tree_area,
            sep: sep_area,
            doc: doc_area,
        };

        match state.focus {
            Panel::Tree => tree_area,
            Panel::Doc => doc_area,
        }
    };

    // Selection highlight (requirement 9.7 "드래그 범위의 텍스트가 반전 표시로
    // 선택"): layered on top of whatever `doc_panel::render` just drew, the
    // same "mutate already-drawn cells" approach the focus marker/border
    // below use, so `doc_panel`'s own signature (locked since task 6.2)
    // stays untouched. `state.layout.doc` is this frame's real doc area
    // regardless of which branch above ran (zeroed when the doc panel
    // wasn't drawn at all, in which case there is nothing to highlight).
    if let Some(sel) = &state.selection {
        apply_selection_highlight(frame, state.layout.doc, state.scroll, sel);
    }

    // Requirement 2.10: the status bar's search segment shows whichever
    // panel's own search is relevant to what is focused right now -- the
    // tree panel's while it is focused, the doc panel's otherwise. Reuses
    // `status_bar::render`'s existing `&SearchState` parameter/rendering
    // path unchanged; only which `SearchState` is passed in differs.
    let active_search = match state.focus {
        Panel::Tree => &state.tree_search,
        Panel::Doc => &state.search,
    };
    status_bar::render(
        frame,
        status_area,
        &state.doc,
        state.scroll,
        active_search,
        &state.watch,
    );

    // Focus marker (requirement 2.7 "활성 패널 시각적 구분"): none of the four
    // panel renderers accept a "focused" parameter (their signatures are
    // locked from tasks 6.1-6.3), so the distinction is drawn here, layered
    // on top after the fact, by overwriting the top-right corner cell of
    // whichever area belongs to the focused panel with a marker glyph
    // distinct from any box-drawing border character. In single-panel mode
    // (width < 80) `focused_area` is `main_area` itself, since the focused
    // panel *is* the only thing rendered -- the marker still lands
    // deterministically at that panel's own top-right corner, which is a
    // meaningful and testable signal rather than a special case to avoid.
    if focused_area.width > 0 {
        frame.buffer_mut().set_string(
            focused_area.x + focused_area.width - 1,
            focused_area.y,
            "●",
            Style::new().fg(Color::Cyan),
        );
    }

    // Focus border recoloring (requirement 2.7): the corner marker above is
    // easy to miss, so also recolor the focused panel's entire border
    // perimeter -- the cells already holding the `┌─┐│└─┘` glyphs that
    // `Block::bordered()` drew inside `tree_panel::render`/`doc_panel::render`
    // -- to a color distinct from the (default-styled) inactive panel's
    // border. This mutates `Cell::fg` directly on cells already drawn by the
    // child panel, the same "layer on top after the fact" approach the
    // marker above already uses, so it needs no change to either panel
    // renderer's signature. Guarded the same way against a degenerate
    // (zero-size) area.
    if focused_area.width > 0 && focused_area.height > 0 {
        let buffer = frame.buffer_mut();
        let left = focused_area.x;
        let right = focused_area.x + focused_area.width - 1;
        let top = focused_area.y;
        let bottom = focused_area.y + focused_area.height - 1;

        // Top and bottom rows, full width.
        for x in left..=right {
            if let Some(cell) = buffer.cell_mut((x, top)) {
                cell.fg = Color::Cyan;
            }
            if top != bottom {
                if let Some(cell) = buffer.cell_mut((x, bottom)) {
                    cell.fg = Color::Cyan;
                }
            }
        }
        // Left and right columns, excluding the corners already handled
        // above.
        if bottom > top + 1 {
            for y in (top + 1)..bottom {
                if let Some(cell) = buffer.cell_mut((left, y)) {
                    cell.fg = Color::Cyan;
                }
                if left != right {
                    if let Some(cell) = buffer.cell_mut((right, y)) {
                        cell.fg = Color::Cyan;
                    }
                }
            }
        }
    }

    // Popup overlay (design.md `popup`): drawn last, on top of everything,
    // over the *full* frame area -- `popup::render` centers itself within
    // whatever area it is given.
    if let Some(popup) = &state.popup {
        popup::render(frame, area, popup, &state.doc);
    }
}

/// Requirement 9.7: reverse-video every visible cell inside `sel`'s
/// (line, col) range. `doc_area` is the *outer* doc panel rect (border
/// included, matching `PanelLayout.doc`); the border itself is excluded
/// via the same 1-cell margin `doc_panel::render`'s own `Block::bordered()`
/// reserves. Lines strictly between the selection's start and end are
/// reversed in full; the start/end lines are only reversed from/to the
/// selection's own column. A degenerate (zero-size) doc area is a no-op.
fn apply_selection_highlight(frame: &mut Frame, doc_area: Rect, scroll: usize, sel: &Selection) {
    let inner = doc_area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let ((start_line, start_col), (end_line, end_col)) = sel.ordered();
    let buffer = frame.buffer_mut();

    for y in inner.y..(inner.y + inner.height) {
        let line = scroll + (y - inner.y) as usize;
        if line < start_line || line > end_line {
            continue;
        }
        let row_start = if line == start_line { start_col } else { 0 };
        let row_end = if line == end_line { end_col } else { inner.width as usize };
        let from = (row_start as u16).min(inner.width);
        let to = (row_end as u16).min(inner.width);
        for x in (inner.x + from)..(inner.x + to) {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.modifier.insert(Modifier::REVERSED);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{self, AppState, DocView, Panel, Popup, TreeMode, WatchStatus};
    use crate::spec::{self, DocKind, NodeId, TreeSource};
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::Terminal;
    use std::path::PathBuf;
    use unicode_width::UnicodeWidthStr;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/kiro"))
    }

    fn build_state(size: (u16, u16)) -> AppState {
        let snapshot = app::load_snapshot(&fixtures_root());
        let root = spec::build(&snapshot);
        AppState::new(
            TreeSource::Kiro(root),
            fixtures_root(),
            size,
            WatchStatus::Live,
            TreeMode::Auto,
            true,
        )
    }

    /// Build a state with `sample-signup`'s tree node expanded and its
    /// requirements.md loaded into the doc panel, so both panels have
    /// distinguishable, known text to search for.
    fn state_with_doc_loaded(size: (u16, u16)) -> AppState {
        let mut state = build_state(size);
        state.tree.open(vec![NodeId::Spec("sample-signup".to_string())]);
        state.tree.select(vec![
            NodeId::Spec("sample-signup".to_string()),
            NodeId::Doc("sample-signup".to_string(), DocKind::Requirements),
        ]);
        let path = fixtures_root().join("specs/sample-signup/requirements.md");
        state.doc = app::load_doc(&path, app::doc_panel_width(&state));
        state
    }

    // Column-width-aware row extraction, matching the pattern used by
    // `doc_panel`/`status_bar`'s own test modules: a wide glyph's hidden
    // continuation cell otherwise reads back as a spurious extra space.
    fn buffer_rows(buffer: &Buffer) -> Vec<String> {
        let area = buffer.area();
        (0..area.height)
            .map(|y| {
                let mut row = String::new();
                let mut x = 0u16;
                while x < area.width {
                    let symbol = buffer.get(x, y).symbol();
                    row.push_str(symbol);
                    let w = UnicodeWidthStr::width(symbol).max(1) as u16;
                    x += w;
                }
                row
            })
            .collect()
    }

    /// The column at which `needle` first appears in `row`, or `None`.
    fn find_col(rows: &[String], needle: &str) -> Option<usize> {
        rows.iter().find_map(|row| row.find(needle))
    }

    fn draw(state: &mut AppState, w: u16, h: u16) -> Buffer {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(frame, state))
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    #[test]
    fn width_120_renders_two_panels_in_distinct_columns() {
        let mut state = state_with_doc_loaded((120, 40));
        let buffer = draw(&mut state, 120, 40);
        let rows = buffer_rows(&buffer);

        // Tree-only text: a spec name from `root.specs`.
        let tree_col = find_col(&rows, "sample-signup")
            .expect("expected sample-signup spec name visible in tree panel");

        // Doc-only text: distinctive text from the loaded requirements.md.
        let doc_needle = match &state.doc {
            DocView::Rendered { r, .. } => {
                let text: String = r.lines[0].spans.iter().map(|s| s.text.as_str()).collect();
                assert!(!text.is_empty(), "fixture doc's first line should not be empty");
                text.chars().take(6).collect::<String>()
            }
            other => panic!("expected Rendered doc, got {other:?}"),
        };
        let doc_col = find_col(&rows, &doc_needle)
            .expect("expected loaded doc's first-line text visible in doc panel");

        // The 30/70 split puts the tree panel's content within the left
        // ~30% of the main area and the doc panel's content to its right.
        let split_col = (120f64 * 0.30) as usize;
        assert!(
            tree_col < split_col,
            "expected tree text at column {tree_col} to be left of the ~30% split at {split_col}"
        );
        assert!(
            doc_col >= split_col,
            "expected doc text at column {doc_col} to be right of the ~30% split at {split_col}"
        );
    }

    // --- task 19.1: TreeMode::Single always renders one full-width panel,
    // regardless of width, keyed off `doc_focus` (not `focus`/width the way
    // Auto's narrow fallback (8.5) is) --------------------------------

    #[test]
    fn single_mode_shows_only_tree_at_a_wide_width_when_doc_not_focused() {
        let mut state = state_with_doc_loaded((120, 40));
        state.tree_mode = TreeMode::Single;
        state.doc_focus = false;
        let buffer = draw(&mut state, 120, 40);
        let rows = buffer_rows(&buffer);

        assert!(
            find_col(&rows, "sample-signup").is_some(),
            "expected tree content visible in Single mode with doc_focus=false"
        );
        let doc_needle = match &state.doc {
            DocView::Rendered { r, .. } => {
                let text: String = r.lines[0].spans.iter().map(|s| s.text.as_str()).collect();
                text.chars().take(6).collect::<String>()
            }
            other => panic!("expected Rendered doc, got {other:?}"),
        };
        assert!(
            find_col(&rows, &doc_needle).is_none(),
            "did not expect doc content when Single mode is showing the tree, even at a wide width"
        );
    }

    #[test]
    fn single_mode_shows_only_doc_at_a_wide_width_when_doc_focused() {
        let mut state = state_with_doc_loaded((120, 40));
        state.tree_mode = TreeMode::Single;
        state.doc_focus = true;
        let buffer = draw(&mut state, 120, 40);
        let rows = buffer_rows(&buffer);

        let doc_needle = match &state.doc {
            DocView::Rendered { r, .. } => {
                let text: String = r.lines[0].spans.iter().map(|s| s.text.as_str()).collect();
                text.chars().take(6).collect::<String>()
            }
            other => panic!("expected Rendered doc, got {other:?}"),
        };
        assert!(
            find_col(&rows, &doc_needle).is_some(),
            "expected doc content visible in Single mode with doc_focus=true, even at a wide width"
        );
        // Not "sample-signup" alone: the status bar (always rendered,
        // regardless of layout mode) shows the doc's path, which itself
        // contains that spec name -- "[implementation]" is the tree panel's
        // own phase-badge syntax (`tree_panel::spec_item`), never emitted
        // anywhere else.
        assert!(
            find_col(&rows, "[implementation]").is_none(),
            "did not expect tree content when Single mode is showing the doc"
        );
    }

    #[test]
    fn width_60_focus_doc_shows_only_doc_panel_full_width() {
        let mut state = state_with_doc_loaded((60, 40));
        state.focus = Panel::Doc;
        let buffer = draw(&mut state, 60, 40);
        let rows = buffer_rows(&buffer);

        let doc_needle = match &state.doc {
            DocView::Rendered { r, .. } => {
                let text: String = r.lines[0].spans.iter().map(|s| s.text.as_str()).collect();
                text.chars().take(6).collect::<String>()
            }
            other => panic!("expected Rendered doc, got {other:?}"),
        };
        assert!(
            find_col(&rows, &doc_needle).is_some(),
            "expected doc content visible when focus is Doc at narrow width"
        );

        // The tree panel must not have rendered at all: its distinguishing
        // spec-name text is absent.
        assert!(
            find_col(&rows, "sample-signup").is_none(),
            "did not expect tree panel content when only the Doc panel is focused/rendered"
        );
    }

    #[test]
    fn width_60_focus_tree_shows_only_tree_panel_full_width() {
        let mut state = state_with_doc_loaded((60, 40));
        state.focus = Panel::Tree;
        let buffer = draw(&mut state, 60, 40);
        let rows = buffer_rows(&buffer);

        assert!(
            find_col(&rows, "sample-signup").is_some(),
            "expected tree panel content visible when focus is Tree at narrow width"
        );

        let doc_needle = match &state.doc {
            DocView::Rendered { r, .. } => {
                let text: String = r.lines[0].spans.iter().map(|s| s.text.as_str()).collect();
                text.chars().take(6).collect::<String>()
            }
            other => panic!("expected Rendered doc, got {other:?}"),
        };
        assert!(
            find_col(&rows, &doc_needle).is_none(),
            "did not expect doc panel content when only the Tree panel is focused/rendered"
        );
    }

    #[test]
    fn render_stores_panel_layout_for_two_panel_mode() {
        // Task 12.1: `state.layout` must reflect exactly what was drawn --
        // tree, a 1-column sep, then doc, filling the main area with no
        // gaps (requirement 9.6's drag handle needs a real, hit-testable
        // column between the two panels).
        let mut state = state_with_doc_loaded((120, 40));
        draw(&mut state, 120, 40);

        let l = state.layout;
        assert!(l.tree.width > 0 && l.sep.width == 1 && l.doc.width > 0);
        assert_eq!(l.tree.x, 0);
        assert_eq!(l.sep.x, l.tree.x + l.tree.width);
        assert_eq!(l.doc.x, l.sep.x + l.sep.width);
        assert_eq!(l.doc.x + l.doc.width, 120, "doc panel should reach the frame's right edge");
        assert_eq!(state.split, l.tree.width, "split should match the rendered tree width");
    }

    // --- task 12.5: text-selection drag + edge auto-scroll (frame half) -

    #[test]
    fn dragging_in_the_doc_panel_reverses_the_selected_cells_next_frame() {
        let mut state = state_with_doc_loaded((120, 40));
        draw(&mut state, 120, 40); // populates state.layout
        let doc_area = state.layout.doc;
        let inner_x = doc_area.x + 1;
        let inner_y = doc_area.y + 1;

        // Select from (row 0, col 2) to (row 1, col 5) of the doc's inner
        // content area -- entirely inside the panel, no edge auto-scroll.
        app::update(
            &mut state,
            app::Action::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: inner_x + 2,
                row: inner_y,
                modifiers: crossterm::event::KeyModifiers::NONE,
            }),
        );
        app::update(
            &mut state,
            app::Action::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left),
                column: inner_x + 5,
                row: inner_y + 1,
                modifiers: crossterm::event::KeyModifiers::NONE,
            }),
        );

        let buffer = draw(&mut state, 120, 40);
        let reversed = |x: u16, y: u16| buffer.get(x, y).modifier.contains(ratatui::style::Modifier::REVERSED);

        // Before the selection starts on the first row: not reversed.
        assert!(!reversed(inner_x, inner_y), "before selection start");
        // Inside the selection on the first row (from col 2 onward): reversed.
        assert!(reversed(inner_x + 3, inner_y), "inside selection, first row");
        // Inside the selection on the second row (before col 5): reversed.
        assert!(reversed(inner_x + 1, inner_y + 1), "inside selection, second row");
        // After the selection ends on the second row (from col 5 onward): not reversed.
        assert!(!reversed(inner_x + 6, inner_y + 1), "after selection end, second row");
        // A row well past the selection: not reversed.
        assert!(!reversed(inner_x + 3, inner_y + 5), "well past the selection");
    }

    #[test]
    fn holding_the_pointer_at_the_bottom_edge_auto_scrolls_on_each_tick() {
        // Task 12.5 DONE text: "포인터가 상/하 경계에 있을 때 Tick마다 1행
        // 스크롤하며 선택이 확장되는 ... 프레임 테스트" -- a real Down inside
        // the doc panel, a Drag to its bottom edge, then repeated Ticks
        // (no further mouse movement) must keep scrolling and extending
        // the selection, and the *rendered* frame must reflect the new
        // scroll position.
        let mut state = state_with_doc_loaded((120, 40));
        // A long enough doc that scrolling is actually observable.
        state.doc = app::DocView::Rendered {
            path: std::path::PathBuf::from("inline.md"),
            r: crate::markdown::render(&"line\n\n".repeat(60), 80),
            meta: app::FileInfo::default(),
        };
        draw(&mut state, 120, 40); // populates state.layout
        let doc_area = state.layout.doc;
        let inner_x = doc_area.x + 1;
        let inner_top = doc_area.y + 1;
        let inner_bottom = doc_area.y + doc_area.height - 2; // last content row

        app::update(
            &mut state,
            app::Action::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: inner_x,
                row: inner_top,
                modifiers: crossterm::event::KeyModifiers::NONE,
            }),
        );
        app::update(
            &mut state,
            app::Action::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left),
                column: inner_x,
                row: inner_bottom,
                modifiers: crossterm::event::KeyModifiers::NONE,
            }),
        );
        assert_eq!(state.auto_scroll, Some(app::AutoScrollDir::Down));

        let scroll_before = state.scroll;
        for _ in 0..3 {
            app::update(&mut state, app::Action::Tick);
        }
        assert_eq!(state.scroll, scroll_before + 3, "expected one row per tick");
        assert_eq!(
            state.selection.unwrap().head.0,
            scroll_before + (inner_bottom - inner_top) as usize + 3,
            "expected the selection head to keep extending past the last visible row"
        );

        let buffer = draw(&mut state, 120, 40);
        // The doc panel's very first visible line after scrolling should
        // show the fixture's repeated "line" text, not the original
        // (now-scrolled-past) opening line -- confirms the frame itself
        // moved, not just internal state.
        let row0: String = (inner_x..inner_x + 10).map(|x| buffer.get(x, inner_top).symbol()).collect();
        assert!(row0.contains("line"), "expected doc content visible after auto-scroll, got {row0:?}");
    }

    #[test]
    fn dragging_the_separator_visibly_moves_the_split_next_frame() {
        // Task 12.4 DONE text: "sep 열 Down->Drag->Up 으로 split 이 바뀌고 ...
        // 프레임 테스트" -- a real Down-on-sep/Drag/Up sequence dispatched
        // through app::update, then the *next* render actually drawing the
        // tree/doc columns at the new width, not just `state.split`
        // changing internally.
        let mut state = state_with_doc_loaded((120, 40));
        draw(&mut state, 120, 40); // first frame: populates state.layout
        let sep_before = state.layout.sep;
        assert!(sep_before.width == 1, "sanity: sep should be exactly 1 column");

        app::update(
            &mut state,
            app::Action::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: sep_before.x,
                row: sep_before.y,
                modifiers: crossterm::event::KeyModifiers::NONE,
            }),
        );
        app::update(
            &mut state,
            app::Action::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left),
                column: sep_before.x + 20,
                row: sep_before.y,
                modifiers: crossterm::event::KeyModifiers::NONE,
            }),
        );
        app::update(
            &mut state,
            app::Action::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                column: sep_before.x + 20,
                row: sep_before.y,
                modifiers: crossterm::event::KeyModifiers::NONE,
            }),
        );
        assert_eq!(state.drag, None);

        draw(&mut state, 120, 40);
        assert_eq!(
            state.layout.sep.x,
            sep_before.x + 20,
            "expected the sep column to have actually moved by the dragged amount"
        );
        assert_eq!(state.layout.tree.width, sep_before.x + 20);
    }

    #[test]
    fn render_stores_zeroed_layout_for_hidden_panels() {
        // Narrow single-panel mode: the panel that is not drawn gets a
        // zero-size Rect so mouse hit-testing (`app::mouse::panel_at`)
        // never matches it.
        let mut state = state_with_doc_loaded((60, 40));
        state.focus = Panel::Doc;
        draw(&mut state, 60, 40);
        assert_eq!(state.layout.tree, Rect::default());
        assert_eq!(state.layout.sep, Rect::default());
        assert!(state.layout.doc.width > 0);

        // Tree hidden entirely (requirement 1.6/1.7).
        let mut state2 = state_with_doc_loaded((120, 40));
        state2.tree_visible = false;
        draw(&mut state2, 120, 40);
        assert_eq!(state2.layout.tree, Rect::default());
        assert_eq!(state2.layout.sep, Rect::default());
        assert!(state2.layout.doc.width > 0);
    }

    // --- task 12.2, end-to-end half: `TreeState::key_up`/`key_down` only
    // move the selection once a real `Tree` widget render has populated
    // their flattened item list, so this half of the arrow-key/wheel
    // behavior needs `draw()`, not a bare reducer call -- see the
    // corresponding reducer-only tests' comments in `app::mod`'s own test
    // module for why they stop at "took the right branch" instead.

    #[test]
    fn arrow_keys_move_tree_selection_when_tree_is_focused() {
        let mut state = state_with_doc_loaded((120, 40));
        state.focus = Panel::Tree;
        draw(&mut state, 120, 40); // populates TreeState's flattened item list
        state.tree.select_first();
        let before = state.tree.selected().to_vec();

        app::update(
            &mut state,
            app::Action::Key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Down,
                crossterm::event::KeyModifiers::NONE,
            )),
        );
        let after = state.tree.selected().to_vec();
        assert_ne!(
            before, after,
            "expected Down to move the tree selection while Tree is focused"
        );
    }

    #[test]
    fn wheel_over_tree_moves_selection_regardless_of_focus() {
        let mut state = state_with_doc_loaded((120, 40));
        state.focus = Panel::Doc; // requirement 9.2: "활성 패널이 아니어도 동작"
        draw(&mut state, 120, 40); // populates TreeState's flattened item list + state.layout
        state.tree.select_first();
        let before = state.tree.selected().to_vec();

        let tree_area = state.layout.tree;
        assert!(tree_area.width > 0, "sanity: tree panel should have rendered");
        app::update(
            &mut state,
            app::Action::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::ScrollDown,
                column: tree_area.x + 1,
                row: tree_area.y + 1,
                modifiers: crossterm::event::KeyModifiers::NONE,
            }),
        );
        let after = state.tree.selected().to_vec();
        assert_ne!(
            before, after,
            "expected wheel-over-tree to move the tree selection even while Doc is focused"
        );
    }

    #[test]
    fn tree_hidden_shows_doc_panel_at_full_width_even_when_wide() {
        // Requirements 1.6/1.7 (task 11.2): `tree_visible = false` -- e.g.
        // file view, or `--tree hidden` -- fills the *whole* main area with
        // the doc panel, not the narrow-width fallback's focus-dependent
        // choice: at 120 columns, two-panel mode would normally still show
        // the tree.
        let mut state = state_with_doc_loaded((120, 40));
        state.tree_visible = false;
        state.focus = Panel::Doc;
        let buffer = draw(&mut state, 120, 40);
        let rows = buffer_rows(&buffer);
        // Exclude the status bar's own bottom row: it always shows the
        // loaded doc's full path (which legitimately contains the spec
        // name "sample-signup"), independent of whether the tree panel
        // itself is rendered.
        let non_status_rows = &rows[..rows.len() - 1];

        assert!(
            find_col(non_status_rows, "sample-signup").is_none(),
            "did not expect tree panel content when tree_visible is false"
        );
        let doc_needle = match &state.doc {
            DocView::Rendered { r, .. } => {
                let text: String = r.lines[0].spans.iter().map(|s| s.text.as_str()).collect();
                text.chars().take(6).collect::<String>()
            }
            other => panic!("expected Rendered doc, got {other:?}"),
        };
        assert!(
            find_col(&rows, &doc_needle).is_some(),
            "expected doc panel content when tree_visible is false"
        );

        // The doc panel's right border should sit at the frame's own right
        // edge (full width), not at the narrower split boundary two-panel
        // mode would use.
        let doc_right_border =
            buffer.get(area_width(&buffer) - 1, 1).symbol().to_string();
        assert!(
            doc_right_border == "│" || doc_right_border == "┐",
            "expected the doc panel's own border at the frame's right edge, got {doc_right_border:?}"
        );
    }

    #[test]
    fn tree_hidden_ignores_focus_and_always_renders_doc() {
        // Even if `focus` were somehow left on `Panel::Tree` while the tree
        // is hidden, there is nothing to focus -- the doc panel must still
        // be what actually renders.
        let mut state = state_with_doc_loaded((120, 40));
        state.tree_visible = false;
        state.focus = Panel::Tree;
        let buffer = draw(&mut state, 120, 40);
        let rows = buffer_rows(&buffer);
        let non_status_rows = &rows[..rows.len() - 1];
        assert!(find_col(non_status_rows, "sample-signup").is_none());
    }

    fn area_width(buffer: &Buffer) -> u16 {
        buffer.area().width
    }

    #[test]
    fn status_bar_always_present_at_both_width_modes() {
        // Width 120 now carries the task-19.3 file info (date · size · 행)
        // after the path; with this long absolute fixture path the trailing
        // `%` segment is clipped at the right edge, so `%`-visibility is
        // asserted in `status_bar`'s own segment test (`file_info_shown_on_
        // wide_status_bar`, which uses a short path). Here at 120 we assert
        // the file-info date segment (`:` appears only inside the date) is
        // actually present. At width 60 the narrow gate drops the file info
        // and the long path alone fills the row, so the assertion is just
        // that the bottom row has status-bar content on it (non-blank), i.e.
        // that `ui::render` did call `status_bar::render` into that row
        // rather than leaving it empty.
        let mut wide = state_with_doc_loaded((120, 40));
        let wide_buffer = draw(&mut wide, 120, 40);
        let wide_bottom = buffer_rows(&wide_buffer).pop().expect("buffer has rows");
        assert!(
            wide_bottom.contains(':'),
            "expected the file-info date segment on the bottom row at width 120, got: {wide_bottom:?}"
        );

        let mut narrow = state_with_doc_loaded((60, 40));
        let narrow_buffer = draw(&mut narrow, 60, 40);
        let narrow_bottom = buffer_rows(&narrow_buffer).pop().expect("buffer has rows");
        assert!(
            !narrow_bottom.trim().is_empty(),
            "expected non-blank status-bar content on the bottom row at width 60"
        );
    }

    #[test]
    fn focus_marker_moves_with_focus() {
        let mut state = state_with_doc_loaded((120, 40));
        state.focus = Panel::Tree;
        let buffer_tree = draw(&mut state, 120, 40);

        let mut state2 = state_with_doc_loaded((120, 40));
        state2.focus = Panel::Doc;
        let buffer_doc = draw(&mut state2, 120, 40);

        // Tree panel's own area top-right corner column: derived the same
        // way `render` computes it (30% split of the 120-wide, 39-high main
        // area after the 1-row status bar reservation).
        let main_area = Rect::new(0, 0, 120, 39);
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(app::TREE_PANEL_PERCENT),
                Constraint::Percentage(DOC_PANEL_PERCENT),
            ])
            .split(main_area);
        let tree_area = horizontal[0];
        let doc_area = horizontal[1];

        let tree_corner_when_tree_focused =
            buffer_tree.get(tree_area.x + tree_area.width - 1, tree_area.y).symbol().to_string();
        let tree_corner_when_doc_focused =
            buffer_doc.get(tree_area.x + tree_area.width - 1, tree_area.y).symbol().to_string();
        let doc_corner_when_doc_focused =
            buffer_doc.get(doc_area.x + doc_area.width - 1, doc_area.y).symbol().to_string();

        assert_eq!(
            tree_corner_when_tree_focused, "●",
            "expected the focus marker at the tree panel's corner when focus is Tree"
        );
        assert_eq!(
            doc_corner_when_doc_focused, "●",
            "expected the focus marker at the doc panel's corner when focus is Doc"
        );
        assert_ne!(
            tree_corner_when_tree_focused, tree_corner_when_doc_focused,
            "expected the tree panel's corner cell to differ depending on which panel is focused"
        );
    }

    #[test]
    fn clicking_a_panel_visibly_moves_the_focus_marker_next_frame() {
        // Task 12.2 DONE text: "패널 클릭 -> 포커스 전환 프레임 단정" -- a real
        // `Action::Mouse` click dispatched through `app::update`, not a
        // hand-set `state.focus`, must change what the *next* rendered
        // frame's focus marker/border look like (mirrors
        // `focus_marker_moves_with_focus` above, but driven by an actual
        // click).
        let mut state = state_with_doc_loaded((120, 40));
        state.focus = Panel::Tree;
        draw(&mut state, 120, 40); // first frame: populates state.layout

        let doc_area = state.layout.doc;
        assert!(doc_area.width > 0, "sanity: doc panel should have rendered");
        app::update(
            &mut state,
            app::Action::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: doc_area.x + 2,
                row: doc_area.y + 2,
                modifiers: crossterm::event::KeyModifiers::NONE,
            }),
        );
        assert_eq!(state.focus, Panel::Doc, "sanity: click should have focused the doc panel");

        let buffer_after_click = draw(&mut state, 120, 40);
        let doc_corner = buffer_after_click
            .get(doc_area.x + doc_area.width - 1, doc_area.y)
            .symbol()
            .to_string();
        assert_eq!(
            doc_corner, "●",
            "expected the focus marker at the doc panel's corner after clicking it"
        );
    }

    // --- task 12.3: tree click select+load / folder toggle -------------

    fn row_of(rows: &[String], needle: &str) -> u16 {
        rows.iter()
            .position(|r| r.contains(needle))
            .unwrap_or_else(|| panic!("expected a row containing {needle:?}, got: {rows:?}")) as u16
    }

    #[test]
    fn clicking_a_doc_node_selects_and_loads_it() {
        let mut state = build_state((120, 40));
        state.tree.open(vec![NodeId::Spec("sample-signup".to_string())]);
        let buffer = draw(&mut state, 120, 40); // populates layout + tree's rendered rows
        let rows = buffer_rows(&buffer);
        let row = row_of(&rows, "requirements.md");
        let col = state.layout.tree.x + 2;

        app::update(
            &mut state,
            app::Action::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: col,
                row,
                modifiers: crossterm::event::KeyModifiers::NONE,
            }),
        );

        assert_eq!(
            state.tree.selected(),
            [
                NodeId::Spec("sample-signup".to_string()),
                NodeId::Doc("sample-signup".to_string(), DocKind::Requirements),
            ]
        );
        match &state.doc {
            DocView::Rendered { path, .. } => {
                assert!(path.ends_with("requirements.md"), "got path {path:?}");
            }
            other => panic!("expected the doc node's own content loaded, got {other:?}"),
        }
    }

    #[test]
    fn clicking_a_folder_node_toggles_it_open_then_closed() {
        let mut state = build_state((120, 40));
        let buffer = draw(&mut state, 120, 40);
        let rows = buffer_rows(&buffer);
        let row = row_of(&rows, "sample-signup [implementation]");
        let col = state.layout.tree.x + 2;
        let click = || {
            app::Action::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: col,
                row,
                modifiers: crossterm::event::KeyModifiers::NONE,
            })
        };

        let spec_path = vec![NodeId::Spec("sample-signup".to_string())];
        assert!(
            !state.tree.opened().contains(&spec_path),
            "sanity: sample-signup should start collapsed"
        );

        app::update(&mut state, click());
        assert_eq!(state.tree.selected(), spec_path.as_slice());
        assert!(
            state.tree.opened().contains(&spec_path),
            "expected the first click to expand the folder"
        );

        // The row layout is unaffected by expansion until the next render
        // -- draw again so the (still-collapsed-until-now) row position is
        // current before clicking a second time at the same coordinate.
        draw(&mut state, 120, 40);
        app::update(&mut state, click());
        assert!(
            !state.tree.opened().contains(&spec_path),
            "expected the second click to collapse the folder again"
        );
    }

    #[test]
    fn popup_overlay_drawn_on_top_when_set() {
        let mut state = state_with_doc_loaded((120, 40));
        state.popup = Some(Popup::Message("검토중".to_string()));
        let buffer = draw(&mut state, 120, 40);
        let rows = buffer_rows(&buffer);
        assert!(
            rows.iter().any(|row| row.contains("검토중")),
            "expected popup message text to be visible on top of the panels"
        );
    }

    #[test]
    fn very_small_size_does_not_panic() {
        let mut state = state_with_doc_loaded((10, 5));
        // The primary assertion is simply that this does not panic.
        let _buffer = draw(&mut state, 10, 5);
    }
}
