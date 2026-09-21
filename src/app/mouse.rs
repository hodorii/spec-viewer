//! Coordinate -> panel / tree-row / document-(line,col) conversion
//! (design.md "mouse.rs # MouseEvent -> Action (hit-test on PanelLayout)";
//! task 12.1 foundation). Pure functions only -- no `AppState` mutation
//! here, so they stay trivially unit-testable; tasks 12.2-12.6 call these
//! from `app::update`'s mouse handling to decide what to actually do.
//!
//! Tree-row hit-testing is *not* reimplemented here: `TreeState::rendered_at`
//! / `click_at` (tui-tree-widget 0.24) already do it correctly against the
//! widget's own `last_area` (set to the tree's *inner*, post-border area on
//! its last render -- see `tree_panel::render`'s `Block::bordered()`), so
//! duplicating that math would only risk drifting out of sync with the
//! widget's real layout.

use crate::spec::NodeId;
use ratatui::layout::{Margin, Position, Rect};
use tui_tree_widget::TreeState;

use super::PanelLayout;

/// Which of the three hit-testable regions a screen coordinate falls in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitPanel {
    Tree,
    Sep,
    Doc,
}

/// Resolve an absolute screen coordinate to a [`HitPanel`], against the
/// panel rectangles `ui::render` stored on `AppState.layout` this frame.
/// `None` when the coordinate is outside all three (e.g. the status bar
/// row, or a popup overlay).
pub fn panel_at(layout: &PanelLayout, x: u16, y: u16) -> Option<HitPanel> {
    let pos = Position::new(x, y);
    if layout.tree.contains(pos) {
        Some(HitPanel::Tree)
    } else if layout.sep.contains(pos) {
        Some(HitPanel::Sep)
    } else if layout.doc.contains(pos) {
        Some(HitPanel::Doc)
    } else {
        None
    }
}

/// Resolve an absolute screen coordinate inside the doc panel to a document
/// `(line, col)`, given the current scroll offset (design.md: "문서는
/// scroll + (y - top)"). `doc_area` is the *outer* rect `ui::render` passed
/// to `doc_panel::render` (border included, matching what is stored in
/// `PanelLayout.doc`) -- content starts one cell in from the top/left per
/// `doc_panel`'s `Block::bordered()`. Returns `None` for a coordinate on
/// the border itself or outside `doc_area` entirely.
pub fn doc_position_at(doc_area: Rect, scroll: usize, x: u16, y: u16) -> Option<(usize, usize)> {
    let inner = doc_area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let pos = Position::new(x, y);
    if !inner.contains(pos) {
        return None;
    }
    let line = scroll + (y - inner.y) as usize;
    let col = (x - inner.x) as usize;
    Some((line, col))
}

/// Like [`doc_position_at`], but clamps `(x, y)` into the doc panel's inner
/// area instead of returning `None` once the pointer has left it --
/// requirement 9.7's drag-selection routinely does, dragging above/below
/// the panel to extend the selection with auto-scroll. Returns `(scroll,
/// 0)` for a degenerate (zero-size) inner area.
pub fn doc_position_clamped(doc_area: Rect, scroll: usize, x: u16, y: u16) -> (usize, usize) {
    let inner = doc_area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    if inner.width == 0 || inner.height == 0 {
        return (scroll, 0);
    }
    let cy = y.clamp(inner.y, inner.y + inner.height - 1);
    let cx = x.clamp(inner.x, inner.x + inner.width - 1);
    let line = scroll + (cy - inner.y) as usize;
    let col = (cx - inner.x) as usize;
    (line, col)
}

/// Which way (if any) `y` sits at or beyond the doc panel's own top/bottom
/// edge -- requirement 9.7's "포인터가 패널 상/하 경계에 닿으면[...]자동
/// 스크롤". `None` while the pointer is strictly inside the visible content
/// rows (or the panel has degenerated to zero height).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocEdge {
    Top,
    Bottom,
}

pub fn doc_edge_at(doc_area: Rect, y: u16) -> Option<DocEdge> {
    let inner = doc_area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    if inner.height == 0 {
        return None;
    }
    if y <= inner.y {
        Some(DocEdge::Top)
    } else if y >= inner.y + inner.height - 1 {
        Some(DocEdge::Bottom)
    } else {
        None
    }
}

/// Thin, testable wrapper around `TreeState::rendered_at` (see module docs
/// for why tree-row math is not reimplemented here).
pub fn tree_identifier_at(tree: &TreeState<NodeId>, x: u16, y: u16) -> Option<&[NodeId]> {
    tree.rendered_at(Position::new(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> PanelLayout {
        PanelLayout {
            tree: Rect::new(0, 0, 30, 39),
            sep: Rect::new(30, 0, 1, 39),
            doc: Rect::new(31, 0, 89, 39),
        }
    }

    #[test]
    fn panel_at_resolves_each_region() {
        let l = layout();
        assert_eq!(panel_at(&l, 5, 10), Some(HitPanel::Tree));
        assert_eq!(panel_at(&l, 30, 10), Some(HitPanel::Sep));
        assert_eq!(panel_at(&l, 60, 10), Some(HitPanel::Doc));
    }

    #[test]
    fn panel_at_outside_all_regions_is_none() {
        let l = layout();
        // Row 39 is the status bar row, one below every panel's height.
        assert_eq!(panel_at(&l, 5, 39), None);
    }

    #[test]
    fn panel_at_zero_size_region_never_matches() {
        // A hidden panel (e.g. tree while `tree_visible == false`) is
        // stored as a zero-size Rect -- a coordinate that would have been
        // inside its old bounds must fall through to whatever panel
        // actually occupies that space now, not falsely match the hidden
        // one.
        let l = PanelLayout {
            tree: Rect::default(),
            sep: Rect::default(),
            doc: Rect::new(0, 0, 120, 39),
        };
        assert_eq!(panel_at(&l, 5, 2), Some(HitPanel::Doc));
    }

    #[test]
    fn doc_position_at_accounts_for_border_and_scroll() {
        let area = Rect::new(10, 5, 40, 20); // outer rect, border included
        // Top-left content cell is (area.x + 1, area.y + 1) = (11, 6).
        assert_eq!(doc_position_at(area, 0, 11, 6), Some((0, 0)));
        // One row down, two columns right, with a nonzero scroll offset.
        assert_eq!(doc_position_at(area, 7, 13, 7), Some((8, 2)));
    }

    #[test]
    fn doc_position_at_on_border_or_outside_is_none() {
        let area = Rect::new(10, 5, 40, 20);
        assert_eq!(doc_position_at(area, 0, 10, 6), None, "left border column");
        assert_eq!(doc_position_at(area, 0, 11, 5), None, "top border row");
        assert_eq!(doc_position_at(area, 0, 200, 200), None, "far outside");
    }

    #[test]
    fn doc_position_clamped_matches_doc_position_at_inside_the_panel() {
        let area = Rect::new(10, 5, 40, 20);
        assert_eq!(doc_position_clamped(area, 7, 13, 7), (8, 2));
    }

    #[test]
    fn doc_position_clamped_pins_to_the_nearest_edge_when_outside() {
        // Inner content area: x in [11, 48], y in [6, 23] (40x20 outer,
        // 1-cell border on every side).
        let area = Rect::new(10, 5, 40, 20);
        // Above the top edge -> top visible line, column still tracked.
        assert_eq!(doc_position_clamped(area, 3, 20, 0), (3, 9));
        // Below the bottom edge -> last visible line (scroll + height-1).
        assert_eq!(doc_position_clamped(area, 3, 20, 999), (3 + 17, 9));
        // Left/right of the panel entirely -> nearest column edge.
        assert_eq!(doc_position_clamped(area, 3, 0, 7), (4, 0));
        assert_eq!(doc_position_clamped(area, 3, 999, 7), (4, 37));
    }

    #[test]
    fn doc_edge_at_detects_top_and_bottom_rows_only() {
        let area = Rect::new(10, 5, 40, 20); // inner y in [6, 23]
        assert_eq!(doc_edge_at(area, 5), Some(DocEdge::Top), "border row counts as at-edge");
        assert_eq!(doc_edge_at(area, 6), Some(DocEdge::Top), "topmost content row");
        assert_eq!(doc_edge_at(area, 14), None, "middle of the panel");
        assert_eq!(doc_edge_at(area, 23), Some(DocEdge::Bottom), "bottommost content row");
        assert_eq!(doc_edge_at(area, 24), Some(DocEdge::Bottom), "border row counts as at-edge");
    }
}
