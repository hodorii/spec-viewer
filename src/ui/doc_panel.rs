//! Document panel: renders the currently-selected `DocView` (design.md
//! "ui — Panels": `doc_panel: lines[scroll..scroll+h]를 행 단위 &Line 렌더, 검색
//! 일치 span 재스타일`; requirements 2.4, 3.6, 6.10 (scroll-slice half), 7.5,
//! 8.3).

use crate::app::{DocView, SearchState};
use crate::markdown::{LineStyle, Rendered, SpanStyle};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line as RLine, Span as RSpan, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

/// Extract the `Rendered` payload carried by a `DocView`, if any. A small
/// local match rather than importing `app::rendered_of` (that helper is
/// private to `app` and not meant to be reused across the crate boundary).
fn rendered_of(doc: &DocView) -> Option<&Rendered> {
    match doc {
        DocView::Rendered { r, .. } => Some(r),
        DocView::Definition { text, .. } => Some(text),
        _ => None,
    }
}

/// Render the document panel into `area`.
pub fn render(frame: &mut Frame, area: Rect, doc: &DocView, scroll: usize, search: &SearchState) {
    if let Some(r) = rendered_of(doc) {
        render_rendered(frame, area, r, scroll, search);
        return;
    }

    let msg: &str = match doc {
        DocView::Empty => "문서를 선택하세요",
        DocView::Missing(_) => "아직 생성되지 않음",
        DocView::Deleted(_) => "파일이 삭제됨",
        DocView::ReadError { msg, .. } => msg,
        DocView::MetaError { msg, .. } => msg,
        DocView::Rendered { .. } | DocView::Definition { .. } => unreachable!("handled above"),
    };

    let block = Block::default().borders(Borders::ALL);
    let para = Paragraph::new(msg).block(block);
    frame.render_widget(para, area);
}

fn span_style(style: SpanStyle) -> Style {
    match style {
        SpanStyle::Plain => Style::default(),
        SpanStyle::Bold => Style::default().add_modifier(Modifier::BOLD),
        SpanStyle::Italic => Style::default().add_modifier(Modifier::ITALIC),
        SpanStyle::Strikethrough => Style::default().add_modifier(Modifier::CROSSED_OUT),
        SpanStyle::Code => Style::default().fg(Color::Yellow),
        SpanStyle::Link => Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED),
        // Already a fully-resolved ratatui Style from a delegated renderer
        // (dg via GraphEngine::render_document) -- pass it straight through.
        SpanStyle::Raw(style) => style,
    }
}

/// Additional line-level style layered on top of each span's own style
/// (merged via `Style::patch`, not a replacement).
fn line_style(style: Option<LineStyle>) -> Style {
    match style {
        Some(LineStyle::Heading(level)) => heading_style(level),
        Some(LineStyle::Quote) => Style::default()
            .add_modifier(Modifier::ITALIC)
            .fg(Color::DarkGray),
        Some(LineStyle::ListItem(_)) | Some(LineStyle::Table) | Some(LineStyle::Plain) | None => {
            Style::default()
        }
    }
}

/// Per-level heading style — bigger headings (H1/H2) get a bolder, more
/// saturated color; smaller ones (H4-H6) fade toward a plain bold gray, so
/// every level is visually distinguishable from its neighbors at a glance
/// (concept ported from mdview's `theme.rs` `heading: [Style; 6]` array —
/// see THIRD_PARTY.md).
fn heading_style(level: u8) -> Style {
    let base = Style::default().add_modifier(Modifier::BOLD);
    match level {
        1 => base.fg(Color::Cyan),
        2 => base.fg(Color::Blue),
        3 => base.fg(Color::Green),
        4 => base.fg(Color::Yellow),
        5 => base.fg(Color::Gray),
        _ => base.fg(Color::DarkGray),
    }
}

fn render_rendered(
    frame: &mut Frame,
    area: Rect,
    r: &Rendered,
    scroll: usize,
    search: &SearchState,
) {
    let height = area.height as usize;
    let total = r.lines.len();
    let start = scroll.min(total);
    let end = (start + height).min(total);

    let current_match_line = search
        .current
        .and_then(|c| search.matches.get(c))
        .copied();

    let mut out_lines: Vec<RLine<'static>> = Vec::with_capacity(end - start);
    for (offset, line) in r.lines[start..end].iter().enumerate() {
        let abs_idx = start + offset;
        let base_line_style = line_style(line.style);

        let mut spans: Vec<RSpan<'static>> = Vec::with_capacity(line.spans.len() + 1);
        if line.indent > 0 {
            spans.push(RSpan::raw(" ".repeat(line.indent as usize)));
        }
        for s in &line.spans {
            let style = span_style(s.style).patch(base_line_style);
            spans.push(RSpan::styled(s.text.clone(), style));
        }

        let mut rline = RLine::from(spans);

        // Search highlighting is intentionally line-granularity, not
        // column-substring: `SearchState.matches` (task 5.3) only records
        // *which lines* matched, not the byte/column range of the match
        // within each line, so highlighting the whole line is the honest
        // reading of the data actually available here — no redundant
        // re-search is performed to recover column precision.
        if search.matches.contains(&abs_idx) {
            let highlight = if current_match_line == Some(abs_idx) {
                Style::default().bg(Color::LightYellow).add_modifier(Modifier::REVERSED)
            } else {
                Style::default().bg(Color::Yellow).fg(Color::Black)
            };
            rline = rline.style(highlight);
        }

        out_lines.push(rline);
    }

    let para = Paragraph::new(Text::from(out_lines)).block(Block::default().borders(Borders::ALL));
    frame.render_widget(para, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{self, DocView, FileInfo, SearchState};
    use crate::spec;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::style::Modifier;
    use ratatui::Terminal;
    use std::path::PathBuf;
    use unicode_width::UnicodeWidthStr;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/kiro"))
    }

    // Wide (e.g. CJK) glyphs occupy two buffer columns: the first cell holds
    // the glyph, the second is a hidden continuation cell whose `symbol()`
    // happens to also read as a plain space (`Cell::EMPTY`'s symbol). Naively
    // concatenating every column's symbol would inject a spurious space
    // after every wide character, so this walks columns by each symbol's
    // actual display width instead of one-at-a-time.
    fn buffer_text(buffer: &Buffer) -> Vec<String> {
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

    fn joined(buffer: &Buffer) -> String {
        buffer_text(buffer).join("\n")
    }

    fn draw(doc: &DocView, scroll: usize, search: &SearchState, w: u16, h: u16) -> Buffer {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render(frame, area, doc, scroll, search);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn requirements_doc_path() -> PathBuf {
        fixtures_root()
            .join("specs/sample-signup/requirements.md")
    }

    // Extract the plain concatenated text of `r.lines[idx]` directly from
    // the styled spans rather than via `r.plain` — `doc_panel::render`
    // slices `r.lines`, so asserting against `r.lines` matches what is
    // actually rendered (both arrays are kept parallel by
    // `markdown::block::push_heading`, but this avoids depending on that).
    fn line_text(r: &crate::markdown::Rendered, idx: usize) -> String {
        r.lines[idx]
            .spans
            .iter()
            .map(|s| s.text.as_str())
            .collect()
    }

    #[test]
    fn rendered_shows_first_lines_at_zero_scroll() {
        let doc = app::load_doc(&requirements_doc_path(), 60);
        let search = SearchState::default();
        let buf = draw(&doc, 0, &search, 60, 20);
        let text = joined(&buf);
        let first_line_text = match &doc {
            DocView::Rendered { r, .. } => line_text(r, 0),
            _ => panic!("expected Rendered"),
        };
        assert!(!first_line_text.is_empty());
        let needle: String = first_line_text.chars().take(6).collect();
        assert!(
            text.contains(&needle),
            "expected first-line text {needle:?} in:\n{text}"
        );
    }

    #[test]
    fn rendered_at_nonzero_scroll_hides_first_line() {
        let doc = app::load_doc(&requirements_doc_path(), 60);
        let total = match &doc {
            DocView::Rendered { r, .. } => r.lines.len(),
            _ => panic!("expected Rendered"),
        };
        assert!(total > 5, "fixture doc too short for this test: {total} lines");
        let search = SearchState::default();
        let first_line_text = match &doc {
            DocView::Rendered { r, .. } => line_text(r, 0),
            _ => unreachable!(),
        };
        let needle: String = first_line_text.chars().take(6).collect();

        // Scroll far enough that the first line's distinctive text is
        // outside the visible window (height 5).
        let scroll = total - 1;
        let buf = draw(&doc, scroll, &search, 60, 5);
        let text = joined(&buf);
        if !needle.trim().is_empty() {
            assert!(
                !text.contains(&needle),
                "did not expect first-line text {needle:?} after scrolling to {scroll} in:\n{text}"
            );
        }
    }

    #[test]
    fn missing_shows_exact_message() {
        let doc = DocView::Missing(PathBuf::from("nope.md"));
        let search = SearchState::default();
        let buf = draw(&doc, 0, &search, 40, 10);
        assert!(joined(&buf).contains("아직 생성되지 않음"));
    }

    #[test]
    fn deleted_shows_exact_message() {
        let doc = DocView::Deleted(PathBuf::from("gone.md"));
        let search = SearchState::default();
        let buf = draw(&doc, 0, &search, 40, 10);
        assert!(joined(&buf).contains("파일이 삭제됨"));
    }

    #[test]
    fn read_error_shows_msg() {
        let doc = DocView::ReadError {
            path: PathBuf::from("x.md"),
            msg: "permission denied (os error 13)".to_string(),
        };
        let search = SearchState::default();
        let buf = draw(&doc, 0, &search, 60, 10);
        assert!(joined(&buf).contains("permission denied"));
    }

    #[test]
    fn meta_error_shows_msg() {
        let doc = DocView::MetaError {
            spec: "sample-signup".to_string(),
            msg: "invalid JSON at line 3".to_string(),
        };
        let search = SearchState::default();
        let buf = draw(&doc, 0, &search, 60, 10);
        assert!(joined(&buf).contains("invalid JSON at line 3"));
    }

    #[test]
    fn search_match_line_has_distinguishable_style() {
        let src = "alpha line\n\nbeta needle line\n\ngamma line";
        let doc = DocView::Rendered {
            path: PathBuf::from("inline.md"),
            r: crate::markdown::render(src, 80),
            meta: FileInfo::default(),
        };
        let search = SearchState {
            query: "needle".to_string(),
            matches: vec![1],
            current: Some(0),
        };
        let buf = draw(&doc, 0, &search, 80, 10);

        let area = buf.area();
        // Row 0 is inside the bordered block's top border; content starts
        // at row 1 given Block::bordered(). Row 1 = line 0 ("alpha line"),
        // row 2 = line 1 ("beta needle line", the match).
        let matched_row = 2u16;
        let other_row = 1u16;
        assert!(matched_row < area.height && other_row < area.height);

        let matched_cell = buf.get(2, matched_row);
        let other_cell = buf.get(2, other_row);
        let differs = matched_cell.bg != other_cell.bg
            || matched_cell.fg != other_cell.fg
            || matched_cell.modifier != other_cell.modifier;
        assert!(
            differs,
            "expected matched line's style to differ from a non-matched line's style"
        );
    }

    #[test]
    fn heading_and_bold_get_bold_modifier() {
        let src = "# Heading\n\n**bold text** and plain";
        let doc = DocView::Rendered {
            path: PathBuf::from("inline.md"),
            r: crate::markdown::render(src, 80),
            meta: FileInfo::default(),
        };
        let search = SearchState::default();
        let buf = draw(&doc, 0, &search, 80, 10);

        let mut any_bold = false;
        let area = buf.area();
        for y in 0..area.height {
            for x in 0..area.width {
                if buf.get(x, y).modifier.contains(Modifier::BOLD) {
                    any_bold = true;
                }
            }
        }
        assert!(any_bold, "expected at least one bold-styled cell");
    }

    // Sanity check that fixture-backed spec::build still works if a future
    // test wants a `Definition` doc instead of `Rendered`.
    #[test]
    fn definition_doc_also_renders_via_rendered_of_path() {
        let snapshot = crate::app::load_snapshot(&fixtures_root());
        let root = spec::build(&snapshot);
        let spec = root
            .specs
            .iter()
            .find(|s| s.name == "sample-signup")
            .expect("fixture spec present");
        let doc = app::load_definition(spec, 60);
        let search = SearchState::default();
        let buf = draw(&doc, 0, &search, 60, 10);
        assert!(!joined(&buf).trim().is_empty());
    }
}
