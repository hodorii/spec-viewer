//! Task 18.2 repro/regression suite — table horizontal overflow (요구 5.4).
//!
//! The defect: `app::loader::load_doc` receives the panel's *outer* width and
//! hands it straight to `markdown::render`.  `doc_panel::render` then draws
//! the result inside `Block::default().borders(Borders::ALL)`, whose inner
//! area is 2 columns narrower.  A table whose rows occupy the full outer width
//! gets its right border clipped by the panel frame.
//!
//! The Technology Stack table (design.md lines 57-68) is the real-world
//! reproduction: its natural column widths (9 + 29 + ~125 display cols)
//! vastly exceed the panel, so column-shrinking logic is always exercised.

mod support;

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::Terminal;
use spec_viewer::app::{self, DocView, SearchState};
use spec_viewer::ui;
use unicode_width::UnicodeWidthStr;

fn design_md_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../.kiro/specs/spec-viewer/design.md")
}

/// Render `design.md` through the full pipeline (load → doc_panel draw) into
/// a `TestBackend` of the given dimensions, returning the final buffer AND the
/// pre-rendered plain lines (extracted from the `Rendered` payload before
/// drawing so we don't need `Clone`).
fn draw_doc_and_plain(width: u16, height: u16) -> (Buffer, Vec<String>) {
    let doc = app::load_doc(&design_md_path(), width);
    let plain = match &doc {
        DocView::Rendered { r, .. } => r.plain.clone(),
        other => panic!("expected Rendered, got {other:?}"),
    };
    let search = SearchState::default();
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ui::doc_panel::render(frame, area_of_terminal(frame), &doc, 0, &search);
        })
        .unwrap();
    (terminal.backend().buffer().clone(), plain)
}

fn area_of_terminal(frame: &ratatui::Frame) -> ratatui::layout::Rect {
    frame.area()
}

// ---------------------------------------------------------------------------
// Table-line helpers
// ---------------------------------------------------------------------------

fn is_table_line(l: &str) -> bool {
    l.starts_with('│') || l.starts_with('┌') || l.starts_with('├') || l.starts_with('└')
}

/// Locate the Technology Stack table in `plain`: the contiguous run of grid
/// lines starting immediately after the "Technology Stack" heading (the first
/// `┌`-rule below it, continuing until the last `└`-rule).
///
/// Anchored on the heading rather than the header row because header text
/// ("Layer") wraps across multiple lines at narrow pane widths.
fn technology_stack_run(plain: &[String]) -> Vec<&str> {
    let heading = plain
        .iter()
        .position(|l| l.trim() == "Technology Stack")
        .expect("design.md 'Technology Stack' heading present in output");
    let first = plain[heading + 1..]
        .iter()
        .position(|l| is_table_line(l))
        .map(|i| heading + 1 + i)
        .expect("table grid follows the 'Technology Stack' heading");
    let mut end = first;
    while end + 1 < plain.len() && is_table_line(&plain[end + 1]) {
        end += 1;
    }
    plain[first..=end].iter().map(|s| s.as_str()).collect()
}

/// Extract the interior text of every visible content row from a bordered
/// doc-panel buffer — each row's string is the cell-column-aware content
/// between the panel's left and right `Borders::ALL` borders (x = 1 onward,
/// stopping at the last column before the panel's right border).
///
/// For a table line to render correctly its interior string must match
/// `plain[idx]` exactly — any clipping by the panel border would truncate it.
fn interior_rows(buf: &Buffer) -> Vec<String> {
    let area = buf.area();
    let mut rows = Vec::new();
    // row 0 is the panel's top border; last row is bottom border.
    for y in 1..area.height.saturating_sub(1) {
        let mut row = String::new();
        let mut x = 1u16; // skip the panel's left border cell
        while x + 1 < area.width {
            let symbol = buf[(x, y)].symbol();
            row.push_str(symbol);
            let w = UnicodeWidthStr::width(symbol).max(1) as u16;
            x += w;
        }
        rows.push(row);
    }
    rows
}

// ---------------------------------------------------------------------------
// Repro — width 100 (fails before the fix)
// ---------------------------------------------------------------------------

/// REQUIREMENT 5.4 REPRO: when `load_doc` is called with `width = 100`, the
/// doc panel's inner area is 98 columns (100 − 2 borders).  The Technology
/// Stack table, if laid out at 100 columns, has data rows occupying
/// `sum(widths) + 3·ncols + 1` columns which equals 99 (under the current
/// proportional-first algorithm at this width) — one column wider than the
/// inner area and therefore clipped.
///
/// The plain output line widths are the *first* thing to exceed 98.  The
/// buffer-level assertion then confirms that each clipped line's interior
/// string is *shorter* than the plain line (its right border truncated by the
/// panel's own `Borders::ALL`).
#[test]
fn technology_stack_table_fits_panel_inner_width_at_100() {
    let width: u16 = 100;
    let inner = (width - 2) as usize;
    let (buf, plain) = draw_doc_and_plain(width, 4000);
    let run = technology_stack_run(&plain);
    assert!(
        run.len() >= 12,
        "expected a full Technology Stack grid (≥ 12 lines), got {}",
        run.len()
    );
    for line in run.iter().copied() {
        assert!(
            UnicodeWidthStr::width(line) <= inner,
            "plain line exceeds panel inner width {inner} at width {width} — table would be clipped:\n  {line:?}\n  width = {}",
            UnicodeWidthStr::width(line)
        );
    }
    let interiors = interior_rows(&buf);
    for line in run.iter().copied() {
        assert!(
            interiors.iter().any(|r| r.as_str() == line),
            "table row was clipped by the doc panel border at width {width} (inner {inner}):\n  expected: {line:?}\n  rendered interior rows:\n  {}",
            interiors.iter().take(run.len() + 10).map(|r| format!("  {r:?}")).collect::<Vec<_>>().join("\n")
        );
    }
}

// ---------------------------------------------------------------------------
// 4-width regression (要求 5.4: 40 / 80 / 100 / 120)
// ---------------------------------------------------------------------------

/// At every standard pane width the Technology Stack table's right border must
/// sit inside the doc panel frame with no clipping.  Plain output widths are
/// checked against `inner = width - 2`; the buffer check confirms full
/// interior rendering.
#[test]
fn table_fits_panel_inner_width_at_40_80_100_120() {
    for width in [40u16, 80, 100, 120] {
        let inner = (width - 2) as usize;
        let (buf, plain) = draw_doc_and_plain(width, 4000);
        let run = technology_stack_run(&plain);
        assert!(
            run.len() >= 12,
            "width {width}: expected a full grid (≥ 12 lines), got {}",
            run.len()
        );
        for line in run.iter().copied() {
            assert!(
                UnicodeWidthStr::width(line) <= inner,
                "width {width}: table line exceeds panel inner width {inner}:\n  {line:?}",
            );
        }
        let interiors = interior_rows(&buf);
        for line in run.iter().copied() {
            assert!(
                interiors.iter().any(|r| r.as_str() == line),
                "width {width}: table row clipped by the doc panel border (inner {inner}):\n  {line:?}",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// sanity: table row overhead math — data rows fit width exactly, rule rows
// are one narrower
// ---------------------------------------------------------------------------

/// Render Technology Stack directly through the pipeline at width 80 and
/// confirm every `│`-starting row has display width exactly equal to `inner`
/// (data rows) or `inner - 1` (rule rows) — the overhead is correct.
#[test]
fn table_rows_have_correct_overhead_at_80() {
    let width: u16 = 80;
    let inner = (width - 2) as usize;
    let (_, plain) = draw_doc_and_plain(width, 4000);
    let run = technology_stack_run(&plain);
    for line in run.iter().copied() {
        let w = UnicodeWidthStr::width(line);
        if line.starts_with('│') {
            assert!(
                w == inner,
                "data row width must be exactly inner ({inner}) at width {width}, got {w}:\n  {line:?}"
            );
        } else {
            // rule rows (┌┬┐ / ├┼┤ / └┴┘) are one column shorter
            assert!(
                w == inner || w == inner - 1,
                "rule row width must be inner or inner-1 at width {width}, got {w}:\n  {line:?}"
            );
        }
    }
}
