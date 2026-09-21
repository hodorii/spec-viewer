//! Shared visual-verification helpers for the `tests/visual_defects.rs`
//! repro suite (task 9.1) and, per its own doc comments, every later
//! per-defect fix task (9.2-9.5) that needs to assert on *actual rendered
//! pixels* in a `ratatui::buffer::Buffer` rather than on intermediate data
//! structures (`Rendered`, `TreeState`, etc.) or on isolated `Style` values.
//!
//! Deliberately kept crate-generic (not tailored to any single defect): the
//! column-width-aware row/cell helpers here are the exact pattern already
//! used by `src/ui/mod.rs`, `src/ui/tree_panel.rs`, `src/ui/doc_panel.rs`,
//! and `tests/app_flow.rs`'s own test modules -- reused verbatim rather than
//! reinvented, so a wide/CJK glyph's hidden continuation cell never reads
//! back as a spurious extra space.
//!
//! Lives at `tests/support/mod.rs` (not `tests/support.rs`) specifically so
//! it is *not* compiled as its own separate test binary -- the standard Rust
//! convention for code shared across multiple integration-test binaries
//! under `tests/`.

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use ratatui::Terminal;
use unicode_width::UnicodeWidthStr;

use spec_viewer::app::{AppState, DocView, FileInfo, SearchState};
use spec_viewer::ui;

/// Render raw markdown `src` through the real pipeline
/// (`markdown::render` -> `DocView::Rendered` -> `ui::doc_panel::render`)
/// into a `TestBackend` of the given size, and return the final buffer.
///
/// This is the primary way 9.2-9.5's repro tests inspect *actual rendered
/// pixels* for a doc-panel-only concern (headings, tables, quotes, ...)
/// without needing a full `AppState`/tree.
#[allow(dead_code)]
pub fn render_markdown_to_buffer(src: &str, width: u16, height: u16) -> Buffer {
    let r = spec_viewer::markdown::render(src, width);
    let doc = DocView::Rendered {
        path: std::path::PathBuf::from("inline.md"),
        r,
        meta: FileInfo::default(),
    };
    let search = SearchState::default();

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            let area = frame.area();
            ui::doc_panel::render(frame, area, &doc, 0, &search);
        })
        .expect("draw");
    terminal.backend().buffer().clone()
}

/// Render a full `AppState` through `ui::render` into a `TestBackend` of the
/// given size, and return the final buffer. Used by defects that depend on
/// the tree/layout (cursor visibility, active-panel border) rather than on
/// the doc panel alone.
#[allow(dead_code)]
pub fn render_app_to_buffer(state: &mut AppState, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| ui::render(frame, state))
        .expect("draw");
    terminal.backend().buffer().clone()
}

/// Column-width-aware row extraction (reuse of the exact pattern already
/// used by `src/ui/mod.rs`, `src/ui/tree_panel.rs`, `src/ui/doc_panel.rs`,
/// and `tests/app_flow.rs`'s own test modules): a wide/CJK glyph's hidden
/// continuation cell otherwise reads back as a spurious extra space if
/// columns were joined one at a time.
#[allow(dead_code)]
pub fn buffer_rows(buffer: &Buffer) -> Vec<String> {
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

/// The `(fg, bg, modifier)` style tuple of the cell at `(x, y)`.
#[allow(dead_code)]
pub fn cell_style(buffer: &Buffer, x: u16, y: u16) -> (Color, Color, Modifier) {
    let cell = &buffer[(x, y)];
    (cell.fg, cell.bg, cell.modifier)
}

/// Position of the first cell (scanning top-to-bottom, using width-aware
/// column stepping) whose row contains `needle`, as `(row, col)`, or `None`.
///
/// `col` is the cell-column index of the first character of `needle` (not a
/// byte offset), consistent with `buffer_rows`' own width-aware stepping --
/// safe to pass straight into `cell_style`/`buffer.get` for wide/CJK text.
#[allow(dead_code)]
pub fn find_text_cell(buffer: &Buffer, needle: &str) -> Option<(u16, u16)> {
    let area = buffer.area();
    for y in 0..area.height {
        let mut col = 0u16;
        let mut x = 0u16;
        // Build the row's cell-column-indexed characters so we can map a
        // `str::find` byte offset in the joined row back to a cell column.
        let mut row = String::new();
        let mut offsets: Vec<u16> = Vec::new();
        while x < area.width {
            let symbol = buffer[(x, y)].symbol();
            offsets.push(col);
            row.push_str(symbol);
            let w = UnicodeWidthStr::width(symbol).max(1) as u16;
            x += w;
            col += 1;
        }
        if let Some(byte_idx) = row.find(needle) {
            // Map the byte index back to the character index, then to the
            // cell column recorded at that character's position.
            let char_idx = row[..byte_idx].chars().count();
            if let Some(&c) = offsets.get(char_idx) {
                return Some((y, c));
            }
        }
    }
    None
}
