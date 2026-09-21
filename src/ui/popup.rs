//! Modal overlay rendering (design.md "ui — Panels": `popup: Clear + 중앙
//! Block — TOC / 도움말 / 검색 입력 / 메시지`; requirements 6.3, 6.4, 6.11).
//!
//! `render` computes its own centered sub-rectangle of the full frame area
//! (requirement 6.3's "화면 중앙에") and always draws `Clear` first so the
//! popup is opaque -- whatever panel content sits underneath cannot bleed
//! through (requirement wording: "중앙에 불투명하게").

use crate::app::{DocView, FileInfo, Popup};
use crate::markdown::Rendered;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

/// Draw `popup` centered over `area` (the full frame area), using `doc` to
/// resolve the heading list for `Popup::Toc` (which only carries the
/// selected index, not the headings themselves -- see design.md `app::Popup`
/// doc comment).
pub fn render(frame: &mut Frame, area: Rect, popup: &Popup, doc: &DocView) {
    let popup_area = centered_rect(60, 60, area);
    frame.render_widget(Clear, popup_area);

    match popup {
        Popup::Toc(selected) => render_toc(frame, popup_area, *selected, doc),
        Popup::Help(entries) => render_help(frame, popup_area, entries),
        Popup::SearchInput(buffer) => render_search_input(frame, popup_area, buffer),
        Popup::Message(msg) => render_message(frame, popup_area, msg),
    }
}

/// Standard ratatui centered-rect helper: `percent_x`/`percent_y` of `area`,
/// centered both ways.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

/// Extract the current doc's rendered payload, if any -- both
/// `DocView::Rendered` and `DocView::Definition` carry one (design.md
/// `app::DocView`).
fn rendered_of(doc: &DocView) -> Option<&Rendered> {
    match doc {
        DocView::Rendered { r, .. } | DocView::Definition { text: r, .. } => Some(r),
        _ => None,
    }
}

fn render_toc(frame: &mut Frame, area: Rect, selected: usize, doc: &DocView) {
    let block = Block::new().borders(Borders::ALL).title("TOC");
    let headings = rendered_of(doc).map(|r| r.headings.as_slice()).unwrap_or(&[]);

    let items: Vec<ListItem> = headings
        .iter()
        .map(|h| {
            let indent = " ".repeat(((h.level.saturating_sub(1)) as usize) * 2);
            ListItem::new(format!("{indent}{}", h.text))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));

    let mut state = ListState::default().with_selected(if headings.is_empty() {
        None
    } else {
        Some(selected)
    });

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_help(frame: &mut Frame, area: Rect, entries: &[(String, String)]) {
    let block = Block::new().borders(Borders::ALL).title("Help");
    let items: Vec<ListItem> = entries
        .iter()
        .map(|(key, desc)| ListItem::new(format!("{key}  {desc}")))
        .collect();
    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_search_input(frame: &mut Frame, area: Rect, buffer: &str) {
    let block = Block::new().borders(Borders::ALL).title("Search");
    let paragraph = Paragraph::new(format!("/{buffer}")).block(block);
    frame.render_widget(paragraph, area);
}

fn render_message(frame: &mut Frame, area: Rect, msg: &str) {
    let block = Block::new().borders(Borders::ALL).title("Message");
    let paragraph = Paragraph::new(msg).block(block);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;
    use std::path::PathBuf;

    fn buffer_text(buffer: &Buffer) -> Vec<String> {
        let area = buffer.area();
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer.get(x, y).symbol())
                    .collect::<String>()
            })
            .collect()
    }

    /// Substring check tolerant of ratatui's wide-character rendering: a
    /// CJK glyph occupies two terminal cells, and the second (continuation)
    /// cell's `symbol()` reads back as `" "` rather than `""` (see
    /// `ratatui_core::buffer::Cell::symbol`), which would otherwise splice
    /// extra spaces into every multi-byte character when cells are
    /// concatenated naively. Stripping whitespace from both sides before
    /// comparing sidesteps that without weakening the assertion for the
    /// ASCII needles used elsewhere in this file.
    fn contains(buffer: &Buffer, needle: &str) -> bool {
        let needle: String = needle.chars().filter(|c| !c.is_whitespace()).collect();
        buffer_text(buffer).iter().any(|line| {
            let stripped: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            stripped.contains(&needle)
        })
    }

    #[test]
    fn opaque_and_centered_over_background() {
        let backend = TestBackend::new(100, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let popup = Popup::Message("hello".to_string());
        let doc = DocView::Empty;

        terminal
            .draw(|frame| {
                let area = frame.area();
                for y in 0..area.height {
                    for x in 0..area.width {
                        frame
                            .buffer_mut()
                            .set_string(x, y, "X", Style::default());
                    }
                }
                render(frame, area, &popup, &doc);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();

        // Corners of the full frame are well outside the centered 60%/60%
        // popup region -- background must survive there.
        assert_eq!(buffer.get(0, 0).symbol(), "X");
        assert_eq!(buffer.get(99, 0).symbol(), "X");
        assert_eq!(buffer.get(0, 39).symbol(), "X");
        assert_eq!(buffer.get(99, 39).symbol(), "X");

        // The center of the frame is inside the popup: it was cleared and
        // overwritten (border or content), so it must not show "X".
        assert_ne!(buffer.get(50, 20).symbol(), "X");
    }

    #[test]
    fn toc_renders_headings_and_highlights_selected_row() {
        let r = crate::markdown::render("# Heading One\nbody\n\n# Heading Two\nbody2\n", 80);
        assert_eq!(r.headings.len(), 2);
        let doc = DocView::Rendered {
            path: PathBuf::from("x.md"),
            r,
            meta: FileInfo::default(),
        };
        let popup = Popup::Toc(1);

        let backend = TestBackend::new(100, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render(frame, area, &popup, &doc);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(contains(buffer, "Heading One"));
        assert!(contains(buffer, "Heading Two"));

        let lines = buffer_text(buffer);
        let (row_one, col_one) = lines
            .iter()
            .enumerate()
            .find_map(|(i, l)| l.find("Heading One").map(|c| (i, c)))
            .expect("first heading row present");
        let (row_two, col_two) = lines
            .iter()
            .enumerate()
            .find_map(|(i, l)| l.find("Heading Two").map(|c| (i, c)))
            .expect("second heading row present");

        let area = buffer.area();
        let cell_one = buffer.get(col_one as u16 + area.x, row_one as u16 + area.y);
        let cell_two = buffer.get(col_two as u16 + area.x, row_two as u16 + area.y);
        assert_ne!(
            (cell_one.fg, cell_one.bg, cell_one.modifier),
            (cell_two.fg, cell_two.bg, cell_two.modifier),
            "selected heading row must be styled differently than an unselected one"
        );
    }

    #[test]
    fn toc_with_no_headings_does_not_panic() {
        let popup = Popup::Toc(0);
        let doc = DocView::Empty;

        let backend = TestBackend::new(100, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render(frame, area, &popup, &doc);
            })
            .unwrap();

        // No panic is the primary assertion; also confirm a bordered box
        // still drew something (the block's border glyphs use non-space,
        // non-"X" symbols like '│'/'─').
        let buffer = terminal.backend().buffer();
        assert!(contains(buffer, "TOC") || contains(buffer, "─"));
    }

    #[test]
    fn help_renders_key_and_description_pairs() {
        let entries = vec![
            ("q".to_string(), "종료".to_string()),
            ("?".to_string(), "도움말".to_string()),
        ];
        let popup = Popup::Help(entries);
        let doc = DocView::Empty;

        let backend = TestBackend::new(100, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render(frame, area, &popup, &doc);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(contains(buffer, "q"));
        assert!(contains(buffer, "종료"));
        assert!(contains(buffer, "?"));
        assert!(contains(buffer, "도움말"));
    }

    #[test]
    fn search_input_renders_slash_prefixed_buffer() {
        let popup = Popup::SearchInput("hello".to_string());
        let doc = DocView::Empty;

        let backend = TestBackend::new(100, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render(frame, area, &popup, &doc);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(contains(buffer, "/hello"));
    }

    #[test]
    fn message_renders_text() {
        let popup = Popup::Message("문제가 발생했습니다".to_string());
        let doc = DocView::Empty;

        let backend = TestBackend::new(100, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render(frame, area, &popup, &doc);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(contains(buffer, "문제가 발생했습니다"));
    }
}
