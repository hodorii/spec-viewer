//! Repro tests proving each of task 9's four doc-panel/tree/layout visual
//! defects (see the task 9.1 report / controller notes) is genuinely real,
//! against the *actual rendered pixels* of a `ratatui::buffer::Buffer` --
//! not just an intermediate data structure or an isolated `Style` value.
//! Each test here is expected to currently FAIL (this file's own job is
//! only to prove that), and is `#[ignore]`d with a comment naming the task
//! that fixes it, so `master` never carries a failing, non-ignored test.
//!
//! Defect 5 (mermaid diagram connector characters, fixed in task 9.5) is
//! deliberately not covered here -- task 9.5 owns building its own
//! canvas-specific test infrastructure on top of `tests/support`.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use spec_viewer::app::{self, Action, AppState, Panel, WatchStatus};
use spec_viewer::spec::{self, DocKind, NodeId, TreeSource};
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;

fn fixtures_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/kiro"))
}

fn build_state(width: u16, height: u16) -> AppState {
    let snapshot = app::load_snapshot(&fixtures_root());
    let root = spec::build(&snapshot);
    AppState::new(
        TreeSource::Kiro(root),
        fixtures_root(),
        (width, height),
        WatchStatus::Live,
        spec_viewer::app::TreeMode::Auto,
        true,
    )
}

// --- defect 1: tree cursor invisible (fixed in task 9.2) -------------------

#[test]
fn tree_cursor_is_visually_highlighted() {
    let mut state = build_state(120, 40);
    state
        .tree
        .open(vec![NodeId::Spec("sample-signup".to_string())]);
    state.tree.select(vec![
        NodeId::Spec("sample-signup".to_string()),
        NodeId::Doc("sample-signup".to_string(), DocKind::Requirements),
    ]);
    state.focus = Panel::Tree;

    let buffer = support::render_app_to_buffer(&mut state, 120, 40);

    // Both rows exist, are non-dim (`DocStatus::Approved`/`Generated`), and
    // carry no progress suffix, so the only thing that should visually
    // distinguish them is the tree's own selection highlight.
    let (sel_row, sel_col) = support::find_text_cell(&buffer, "requirements.md")
        .expect("selected requirements.md row visible in the tree panel");
    let (other_row, other_col) = support::find_text_cell(&buffer, "biz-process.md")
        .expect("unselected biz-process.md row visible in the tree panel");

    let selected_style = support::cell_style(&buffer, sel_col, sel_row);
    let other_style = support::cell_style(&buffer, other_col, other_row);

    assert_ne!(
        selected_style, other_style,
        "expected the selected tree row's style to differ from an unselected row's \
         style (TreeState's selection currently has zero visual effect)"
    );
}

// --- defect 2: active panel border doesn't visually differ (task 9.2) -----

#[test]
fn active_panel_border_color_differs_from_inactive() {
    let mut state_tree_focus = build_state(120, 40);
    state_tree_focus.focus = Panel::Tree;
    let buf_tree = support::render_app_to_buffer(&mut state_tree_focus, 120, 40);

    let mut state_doc_focus = build_state(120, 40);
    state_doc_focus.focus = Panel::Doc;
    let buf_doc = support::render_app_to_buffer(&mut state_doc_focus, 120, 40);

    // Tree panel area geometry mirrors `ui::render`'s own layout exactly
    // (one row reserved for the status bar, then a 30/70 horizontal split;
    // see `src/ui/mod.rs`'s `TREE_PANEL_PERCENT`/`DOC_PANEL_PERCENT`, which
    // are private to that module and so re-derived here by value).
    let main_area = Rect::new(0, 0, 120, 39);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(main_area);
    let tree_area = horizontal[0];

    // A left-border cell well away from the top-right corner, which already
    // carries task 6.4's small "●" focus marker -- this is testing the
    // *border itself*, not that pre-existing corner marker.
    let border_x = tree_area.x;
    let border_y = tree_area.y + 2;

    let style_when_tree_focused = support::cell_style(&buf_tree, border_x, border_y);
    let style_when_doc_focused = support::cell_style(&buf_doc, border_x, border_y);

    assert_ne!(
        style_when_tree_focused, style_when_doc_focused,
        "expected the tree panel's border style to differ depending on whether it is \
         focused (only the corner marker currently changes; the border itself does not)"
    );
}

// --- defect 3: heading levels render identically (fixed in task 9.3) ------

#[test]
fn heading_levels_have_distinct_styles() {
    let src = "# H1\n\nbody\n\n## H2\n\nbody\n\n### H3\n\nbody\n";
    let buffer = support::render_markdown_to_buffer(src, 80, 30);

    let (h1_row, h1_col) =
        support::find_text_cell(&buffer, "H1").expect("H1 heading visible in the buffer");
    let (h2_row, h2_col) =
        support::find_text_cell(&buffer, "H2").expect("H2 heading visible in the buffer");
    let (h3_row, h3_col) =
        support::find_text_cell(&buffer, "H3").expect("H3 heading visible in the buffer");

    let h1_style = support::cell_style(&buffer, h1_col, h1_row);
    let h2_style = support::cell_style(&buffer, h2_col, h2_row);
    let h3_style = support::cell_style(&buffer, h3_col, h3_row);

    assert_ne!(
        h1_style, h2_style,
        "expected H1 and H2 heading styles to differ (doc_panel::line_style matches \
         Heading(_) uniformly today)"
    );
    assert_ne!(
        h2_style, h3_style,
        "expected H2 and H3 heading styles to differ (doc_panel::line_style matches \
         Heading(_) uniformly today)"
    );
    assert_ne!(
        h1_style, h3_style,
        "expected H1 and H3 heading styles to differ (doc_panel::line_style matches \
         Heading(_) uniformly today)"
    );
}

// --- regression: emphasis span styles remain pairwise distinct (task 9.3) -

#[test]
fn emphasis_styles_remain_pairwise_distinct() {
    // Bold/Italic/Strikethrough/Code/Link must each still carry their own
    // distinct style (already implemented in doc_panel::span_style since
    // task 6.2 — this is a regression guard for this task's DONE text, not
    // new behavior).
    let src = "**bold** *italic* ~~strike~~ `code` [link](https://x.com)";
    let buffer = support::render_markdown_to_buffer(src, 80, 10);

    let (bold_row, bold_col) =
        support::find_text_cell(&buffer, "bold").expect("bold text visible in the buffer");
    let (italic_row, italic_col) =
        support::find_text_cell(&buffer, "italic").expect("italic text visible in the buffer");
    let (strike_row, strike_col) =
        support::find_text_cell(&buffer, "strike").expect("strike text visible in the buffer");
    let (code_row, code_col) =
        support::find_text_cell(&buffer, "code").expect("code text visible in the buffer");
    let (link_row, link_col) =
        support::find_text_cell(&buffer, "link").expect("link text visible in the buffer");

    let bold_style = support::cell_style(&buffer, bold_col, bold_row);
    let italic_style = support::cell_style(&buffer, italic_col, italic_row);
    let strike_style = support::cell_style(&buffer, strike_col, strike_row);
    let code_style = support::cell_style(&buffer, code_col, code_row);
    let link_style = support::cell_style(&buffer, link_col, link_row);

    let styles = [
        ("bold", bold_style),
        ("italic", italic_style),
        ("strike", strike_style),
        ("code", code_style),
        ("link", link_style),
    ];

    for i in 0..styles.len() {
        for j in (i + 1)..styles.len() {
            let (name_a, style_a) = styles[i];
            let (name_b, style_b) = styles[j];
            assert_ne!(
                style_a, style_b,
                "expected {name_a} and {name_b} span styles to differ"
            );
        }
    }
}

// --- defect 4: tables render with no border characters (fixed in task 9.4)

#[test]
fn table_has_border_characters() {
    let src = "| A | B |\n| --- | --- |\n| 1 | 2 |\n";
    let buffer = support::render_markdown_to_buffer(src, 40, 20);
    let rows = support::buffer_rows(&buffer);

    // Exclude the doc panel's own outer `Block::bordered()` frame (its top
    // and bottom row, and each remaining row's leftmost/rightmost column) --
    // that frame is drawn by `doc_panel::render` regardless of content and
    // already contains every one of these box-drawing characters, so a bare
    // "does this character appear anywhere in the buffer" check would
    // trivially pass today without the table drawing anything at all. Only
    // the panel's *interior* is evidence the table itself drew a border.
    let interior: String = rows[1..rows.len().saturating_sub(1)]
        .iter()
        .map(|row| {
            let chars: Vec<char> = row.chars().collect();
            if chars.len() <= 2 {
                String::new()
            } else {
                chars[1..chars.len() - 1].iter().collect::<String>()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    for ch in ['┌', '─', '│', '└'] {
        assert!(
            interior.contains(ch),
            "expected table border character {ch:?} inside the doc panel's interior \
             (render_table currently only emits space-padded columns), got interior:\n{interior}"
        );
    }
}

// --- defect: 2-panel doc wraps at terminal width, not panel width (task 20.1)

/// tasks.md 20's own NO-GO report, against the real repo `.kiro` root (not
/// `tests/fixtures/kiro`) so this is the exact real-world repro rather than
/// a stand-in: in a 120-wide two-panel frame, this spec's own design.md
/// `## 정의` paragraph must be seen loading via the tree (Enter, mirroring a
/// real session) and wrap inside the doc panel's actual (narrower than the
/// terminal) width -- not the full terminal width, which overflows past the
/// panel's right border and gets clipped, silently dropping whole
/// mid-paragraph words/lines rather than just visually truncating them.
/// Loads the real repo's own `.kiro/specs/spec-viewer/design.md` via the
/// tree (Enter, mirroring a real session) at `width`x35 with the given
/// `tree_mode`/`tree_visible`, renders one frame, and asserts every
/// whitespace-separated word of the `## 정의` paragraph is both present
/// (not silently dropped by a mid-paragraph wrap past the panel) and inside
/// the doc panel's actual rendered column range.
fn assert_definition_paragraph_wraps_inside_doc_panel(
    width: u16,
    tree_mode: spec_viewer::app::TreeMode,
    tree_visible: bool,
) {
    let repo_kiro_root = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.kiro"));
    let snapshot = app::load_snapshot(&repo_kiro_root);
    let root = spec::build(&snapshot);
    let mut state = AppState::new(
        TreeSource::Kiro(root),
        repo_kiro_root,
        (width, 35),
        WatchStatus::Live,
        tree_mode,
        tree_visible,
    );
    state.tree.open(vec![NodeId::Spec("spec-viewer".to_string())]);
    state.tree.select(vec![
        NodeId::Spec("spec-viewer".to_string()),
        NodeId::Doc("spec-viewer".to_string(), DocKind::Design),
    ]);
    app::update(&mut state, Action::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    // `TreeMode::Single` starts on the tree panel -- switch to the doc panel
    // so it's actually the one on screen this frame.
    if state.tree_mode == spec_viewer::app::TreeMode::Single {
        state.doc_focus = true;
        state.focus = Panel::Doc;
    }

    let buffer = support::render_app_to_buffer(&mut state, width, 35);
    let doc = state.layout.doc;
    assert!(doc.width > 0, "sanity: doc panel should have rendered at width {width}");

    let paragraph = "개발자를 위해 파일시스템 → 스펙 모델 → 마크다운 렌더 → \
                      화면의 단방향 파이프라인으로 spec-viewer를 나누고 잇는 아키텍처 정의서이다.";
    for word in paragraph.split_whitespace() {
        let (_, col) = support::find_text_cell(&buffer, word)
            .unwrap_or_else(|| panic!("at width {width}: word {word:?} missing entirely from the \
                                        rendered frame -- wrapped past the doc panel and clipped away"));
        let word_width = UnicodeWidthStr::width(word) as u16;
        assert!(
            col >= doc.x && col + word_width <= doc.x + doc.width,
            "at width {width}: word {word:?} at column {col} (width {word_width}) is outside \
             the doc panel's inner column range [{}, {})",
            doc.x,
            doc.x + doc.width
        );
    }
}

#[test]
fn two_panel_doc_wraps_within_panel_width_not_terminal_width() {
    for width in [80, 120] {
        assert_definition_paragraph_wraps_inside_doc_panel(width, spec_viewer::app::TreeMode::Auto, true);
    }
}

#[test]
fn narrow_single_panel_doc_wraps_within_full_main_width() {
    // Below `NARROW_WIDTH_THRESHOLD` (80), `ui::render` shows only the
    // focused panel at the full main-area width -- no tree/sep split to
    // shrink the doc panel by, so this width class was never the bug, but
    // must keep working post-fix.
    assert_definition_paragraph_wraps_inside_doc_panel(40, spec_viewer::app::TreeMode::Single, true);
}

#[test]
fn tree_hidden_doc_wraps_within_full_terminal_width() {
    // `--tree hidden`: reported in tasks.md 20.1 as already-correct
    // ("정상") -- kept here as a regression guard against the fix above.
    assert_definition_paragraph_wraps_inside_doc_panel(120, spec_viewer::app::TreeMode::Hidden, false);
}
