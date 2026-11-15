use crate::markdown::wrap::{line_width, wrap_spans};
use crate::markdown::{HeadingAnchor, Line, LineStyle, Rendered, Span, SpanStyle};
use crate::app::navigator::{link_values, NUMBER_RANGE_SEPARATOR};
use crate::spec::methodology::{LinkRule, LocateBy, MethodologySettings};
use crate::spec::tasks::{is_task_number, TaskItem, TaskState, TasksBlock, TasksDocument};
use ratatui::style::{Color, Modifier, Style};

/// One followable value of a task tag. `line` indexes `Rendered::plain` and
/// `column` is the display-cell offset of the value inside that line's text
/// (indent excluded); both are recomputed on every render because wrapping moves them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLink {
    pub tag_name: String,
    pub value: String,
    pub line: usize,
    pub column: usize,
    pub width: usize,
}

pub struct TasksView {
    pub links: Vec<TaskLink>,
    pub selected_link: Option<usize>,
    document: TasksDocument,
    settings: MethodologySettings,
}

const LAYOUT_WIDTH_BEFORE_FIRST_RENDER: u16 = 80;
const SUB_TASK_INDENT: u16 = 2;
const DETAIL_INDENT_FROM_TASK: u16 = 4;
const MAJOR_TASK_ANCHOR_LEVEL: u8 = 2;
const NO_TASKS_NOTICE: &str = "작업 항목이 없습니다 (체크박스 줄을 찾지 못함)";
const PARALLEL_LABEL: &str = "(P 병렬)";
const OPTIONAL_OPEN_BOX: &str = "[ ]*";
const OPEN_BOX: &str = "[ ]";
const DONE_BOX: &str = "[x]";

fn label_style() -> Style {
    Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)
}

/// The panel's link look plus a reverse-video flip so the chosen link stands out.
fn selected_link_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::UNDERLINED | Modifier::REVERSED)
}

fn state_box(state: TaskState) -> Span {
    let (text, color) = match state {
        TaskState::Done => (DONE_BOX, Color::Green),
        TaskState::Open => (OPEN_BOX, Color::Reset),
        TaskState::OptionalOpen => (OPTIONAL_OPEN_BOX, Color::Yellow),
    };
    Span { text: text.to_string(), style: SpanStyle::Raw(Style::default().fg(color)) }
}

fn plain_span(text: &str) -> Span {
    Span { text: text.to_string(), style: SpanStyle::Plain }
}

/// A number rule only links values that look like a task or clause number
/// (`1.1`, `2.1~2.5`); anything else, such as a component name, stays text.
fn is_followable(rule: &LinkRule, value: &str) -> bool {
    match rule.locate {
        LocateBy::TitleText => !value.is_empty(),
        LocateBy::LeadingNumber => {
            let mut ends = value.split(NUMBER_RANGE_SEPARATOR);
            let count = ends.clone().count();
            count <= 2 && ends.all(is_task_number)
        }
    }
}

struct Token {
    text: String,
    style: SpanStyle,
    link: Option<(String, String)>,
}

struct Layout<'a> {
    width: u16,
    selected_link: Option<usize>,
    settings: &'a MethodologySettings,
    lines: Vec<Line>,
    plain: Vec<String>,
    headings: Vec<HeadingAnchor>,
    links: Vec<TaskLink>,
}

impl<'a> Layout<'a> {
    fn new(width: u16, selected_link: Option<usize>, settings: &'a MethodologySettings) -> Self {
        Self {
            width,
            selected_link,
            settings,
            lines: Vec::new(),
            plain: Vec::new(),
            headings: Vec::new(),
            links: Vec::new(),
        }
    }

    fn push_line(&mut self, indent: u16, spans: Vec<Span>, style: Option<LineStyle>) -> usize {
        self.plain.push(spans.iter().map(|span| span.text.as_str()).collect());
        self.lines.push(Line { indent, spans, style });
        self.lines.len() - 1
    }

    fn push_blank_separator(&mut self) {
        if self.plain.last().is_some_and(|last| !last.is_empty()) {
            self.push_line(0, Vec::new(), None);
        }
    }

    /// Returns the index of the first wrapped line, if anything was written.
    fn push_wrapped(&mut self, spans: &[Span], indent: u16, style: Option<LineStyle>) -> Option<usize> {
        let mut first_line = None;
        for wrapped in wrap_spans(spans, self.width, indent) {
            let line = self.push_line(indent, wrapped, style);
            first_line.get_or_insert(line);
        }
        first_line
    }

    fn push_document(&mut self, document: &TasksDocument) {
        let mut has_task = false;
        for block in &document.blocks {
            match block {
                TasksBlock::Heading { level, text } => {
                    self.push_blank_separator();
                    if let Some(line) =
                        self.push_wrapped(&[plain_span(text)], 0, Some(LineStyle::Heading(*level)))
                    {
                        self.headings.push(HeadingAnchor { level: *level, text: text.clone(), line });
                    }
                }
                TasksBlock::Text(text) => {
                    self.push_wrapped(&[plain_span(text)], 0, Some(LineStyle::Plain));
                }
                TasksBlock::Task(task) => {
                    has_task = true;
                    self.push_task(task);
                }
            }
        }
        if !has_task {
            self.push_blank_separator();
            let notice = Span { text: NO_TASKS_NOTICE.to_string(), style: SpanStyle::Italic };
            self.push_line(0, vec![notice], Some(LineStyle::Plain));
        }
    }

    fn push_task(&mut self, task: &TaskItem) {
        let is_major = task.number.as_deref().is_some_and(|number| !number.contains('.'));
        let indent = if task.number.is_none() || is_major { 0 } else { SUB_TASK_INDENT };
        if is_major {
            self.push_blank_separator();
        }
        let mut spans = vec![state_box(task.state)];
        if let Some(number) = &task.number {
            spans.push(Span { text: number.clone(), style: SpanStyle::Bold });
        }
        if task.is_parallel {
            let style = SpanStyle::Raw(Style::default().fg(Color::Magenta));
            spans.push(Span { text: PARALLEL_LABEL.to_string(), style });
        }
        spans.push(plain_span(&task.title));
        let first_line = self.push_wrapped(&spans, indent, Some(LineStyle::Plain));
        if let (true, Some(line), Some(number)) = (is_major, first_line, &task.number) {
            let text = format!("{number} {}", task.title);
            self.headings.push(HeadingAnchor { level: MAJOR_TASK_ANCHOR_LEVEL, text, line });
        }

        let detail_indent = indent + DETAIL_INDENT_FROM_TASK;
        for note in &task.notes {
            let note_spans = [plain_span(&format!("• {note}"))];
            self.push_wrapped(&note_spans, detail_indent, Some(LineStyle::Plain));
        }
        for tag in &task.tags {
            let tokens = self.tag_tokens(&tag.name, &tag.value);
            self.push_tokens(&tokens, detail_indent);
        }
    }

    fn tag_tokens(&self, tag_name: &str, raw_value: &str) -> Vec<Token> {
        let mut tokens = vec![Token {
            text: format!("{tag_name}:"),
            style: SpanStyle::Raw(label_style()),
            link: None,
        }];
        let rule = self.settings.rule_for(tag_name).and_then(|rule| rule.link.as_ref());
        let Some(rule) = rule else {
            tokens.extend(words_as_tokens(raw_value));
            return tokens;
        };
        for value in link_values(rule, raw_value) {
            if is_followable(rule, &value) {
                tokens.push(Token {
                    text: value.clone(),
                    style: SpanStyle::Link,
                    link: Some((tag_name.to_string(), value)),
                });
            } else {
                tokens.extend(words_as_tokens(&value));
            }
        }
        tokens
    }

    /// Greedy line packing that keeps every token whole, so a link value is never
    /// cut by wrapping, and records where each link value landed.
    fn push_tokens(&mut self, tokens: &[Token], indent: u16) {
        let max_width = usize::from(self.width.saturating_sub(indent).max(1));
        let mut spans: Vec<Span> = Vec::new();
        let mut used_width = 0usize;
        let mut pending_links: Vec<(String, String, usize, usize)> = Vec::new();
        for token in tokens {
            let token_width = line_width(&token.text);
            if token_width == 0 {
                continue;
            }
            let separator_width = usize::from(!spans.is_empty());
            if !spans.is_empty() && used_width + separator_width + token_width > max_width {
                self.flush_packed_line(&mut spans, &mut pending_links, indent);
                used_width = 0;
            }
            if !spans.is_empty() {
                spans.push(plain_span(" "));
                used_width += 1;
            }
            if let Some((tag_name, value)) = &token.link {
                pending_links.push((tag_name.clone(), value.clone(), used_width, token_width));
            }
            let link_number = self.links.len() + pending_links.len();
            let style = match (&token.link, self.selected_link) {
                (Some(_), Some(selected)) if selected + 1 == link_number => {
                    SpanStyle::Raw(selected_link_style())
                }
                _ => token.style,
            };
            spans.push(Span { text: token.text.clone(), style });
            used_width += token_width;
        }
        self.flush_packed_line(&mut spans, &mut pending_links, indent);
    }

    fn flush_packed_line(
        &mut self,
        spans: &mut Vec<Span>,
        pending_links: &mut Vec<(String, String, usize, usize)>,
        indent: u16,
    ) {
        if spans.is_empty() {
            return;
        }
        let line = self.push_line(indent, std::mem::take(spans), Some(LineStyle::Plain));
        for (tag_name, value, column, width) in pending_links.drain(..) {
            self.links.push(TaskLink { tag_name, value, line, column, width });
        }
    }
}

fn words_as_tokens(text: &str) -> impl Iterator<Item = Token> + '_ {
    text.split_whitespace()
        .map(|word| Token { text: word.to_string(), style: SpanStyle::Plain, link: None })
}

impl TasksView {
    pub fn build(document: TasksDocument, settings: &MethodologySettings) -> Self {
        let mut view = Self { links: Vec::new(), selected_link: None, document, settings: settings.clone() };
        view.links = view.lay_out(LAYOUT_WIDTH_BEFORE_FIRST_RENDER).1;
        view
    }

    /// Re-lays the view out for `width`. Link positions change with the width but
    /// their order and count do not, so the selected link survives a re-render.
    pub fn render(&mut self, width: u16) -> Rendered {
        let (rendered, links) = self.lay_out(width);
        self.links = links;
        rendered
    }

    /// Next link after the selected one; with none selected, the first link at or
    /// below `from_line`. Returns the newly selected index, `None` when nothing moved.
    pub fn select_next_link(&mut self, from_line: usize) -> Option<usize> {
        let next = match self.selected_link {
            Some(current) => Some(current + 1).filter(|&index| index < self.links.len()),
            None => self.links.iter().position(|link| link.line >= from_line),
        }?;
        self.selected_link = Some(next);
        Some(next)
    }

    /// Previous link before the selected one; with none selected, the last link
    /// above `from_line`. Returns the newly selected index, `None` when nothing moved.
    pub fn select_previous_link(&mut self, from_line: usize) -> Option<usize> {
        let previous = match self.selected_link {
            Some(current) => current.checked_sub(1),
            None => self.links.iter().rposition(|link| link.line < from_line),
        }?;
        self.selected_link = Some(previous);
        Some(previous)
    }

    pub fn selected(&self) -> Option<&TaskLink> {
        self.selected_link.and_then(|index| self.links.get(index))
    }

    fn lay_out(&self, width: u16) -> (Rendered, Vec<TaskLink>) {
        let mut layout = Layout::new(width, self.selected_link, &self.settings);
        layout.push_document(&self.document);
        let rendered = Rendered {
            lines: layout.lines,
            plain: layout.plain,
            headings: layout.headings,
            footnotes: Vec::new(),
        };
        (rendered, layout.links)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::SearchState;
    use crate::app::DocView;
    use crate::spec::tasks::parse_tasks;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::Terminal;
    use unicode_width::UnicodeWidthStr;

    const SAMPLE: &str =
        include_str!("../../tests/fixtures/kiro-tasks-view/specs/tasks-view-sample/tasks.md");

    fn view_of(source: &str) -> TasksView {
        let settings = MethodologySettings::kiro_default();
        TasksView::build(parse_tasks(source, &settings), &settings)
    }

    fn sample_view() -> TasksView {
        view_of(SAMPLE)
    }

    fn draw(rendered: Rendered, width: u16, height: u16) -> Buffer {
        let doc = DocView::Definition { spec: "sample".into(), text: rendered };
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                crate::ui::doc_panel::render(frame, area, &doc, 0, &SearchState::default());
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    /// Rows as text plus, per row, the buffer column where each char starts.
    fn rows_with_columns(buffer: &Buffer) -> Vec<(String, Vec<u16>)> {
        let area = buffer.area();
        (0..area.height)
            .map(|y| {
                let mut text = String::new();
                let mut columns = Vec::new();
                let mut x = 0u16;
                while x < area.width {
                    let symbol = buffer[(x, y)].symbol();
                    for _ in symbol.chars() {
                        columns.push(x);
                    }
                    text.push_str(symbol);
                    x += UnicodeWidthStr::width(symbol).max(1) as u16;
                }
                (text, columns)
            })
            .collect()
    }

    /// Buffer position (x, y) of the first occurrence of `needle`.
    fn find_on_screen(buffer: &Buffer, needle: &str) -> Option<(u16, u16)> {
        for (y, (text, columns)) in rows_with_columns(buffer).into_iter().enumerate() {
            if let Some(byte_index) = text.find(needle) {
                let char_index = text[..byte_index].chars().count();
                return Some((columns[char_index], y as u16));
            }
        }
        None
    }

    fn style_at(buffer: &Buffer, position: (u16, u16)) -> Style {
        buffer[position].style()
    }

    fn is_link_look(style: Style) -> bool {
        style.fg == Some(Color::Cyan) && style.add_modifier.contains(Modifier::UNDERLINED)
    }

    fn is_selected_look(style: Style) -> bool {
        style.add_modifier.contains(Modifier::REVERSED)
    }

    #[test]
    fn frame_shows_task_lines_with_state_number_and_title() {
        let mut view = sample_view();
        let buffer = draw(view.render(80), 80, 40);
        assert!(find_on_screen(&buffer, "[x]").is_some());
        let (_, row) = find_on_screen(&buffer, "1.1").expect("number 1.1");
        let line = &rows_with_columns(&buffer)[row as usize].0;
        assert!(line.contains("[x]"), "{line}");
        assert!(line.contains("가입 양식 검증"), "{line}");
        let (_, open_row) = find_on_screen(&buffer, "2.1 ").expect("task 2.1");
        assert!(rows_with_columns(&buffer)[open_row as usize].0.contains("[ ]"));
    }

    #[test]
    fn optional_open_task_is_marked_optional() {
        let mut view = sample_view();
        let buffer = draw(view.render(80), 80, 40);
        let (_, row) = find_on_screen(&buffer, "탈퇴 확인 절차").expect("task 3.1");
        assert!(rows_with_columns(&buffer)[row as usize].0.contains("[ ]*"));
    }

    #[test]
    fn frame_shows_parallel_marker_only_on_parallel_tasks() {
        let mut view = sample_view();
        let buffer = draw(view.render(80), 80, 40);
        let (_, row) = find_on_screen(&buffer, "가입 양식 검증").unwrap();
        assert!(rows_with_columns(&buffer)[row as usize].0.contains("(P"), "marker on 1.1");
        let (_, serial_row) = find_on_screen(&buffer, "세션 발급").unwrap();
        assert!(!rows_with_columns(&buffer)[serial_row as usize].0.contains("(P"));
    }

    #[test]
    fn tag_sublines_are_below_the_task_line_and_indented_past_the_title() {
        let mut view = sample_view();
        let buffer = draw(view.render(80), 80, 40);
        let (task_x, task_row) = find_on_screen(&buffer, "[ ] 2.1").unwrap();
        let (label_x, label_row) = find_on_screen(&buffer, "Depends").expect("Depends label");
        assert!(label_row > task_row);
        assert!(label_x > task_x, "subline indents deeper than the task line");
        let rows = rows_with_columns(&buffer);
        assert!(rows[label_row as usize].0.contains("2.1~2.3") || find_on_screen(&buffer, "2.1~2.3").is_some());
    }

    #[test]
    fn done_alias_renders_as_done_when_subline() {
        let mut view = sample_view();
        let buffer = draw(view.render(80), 80, 40);
        let (_, row) = find_on_screen(&buffer, "새 계정이 저장소에 기록된다").expect("alias text");
        assert!(rows_with_columns(&buffer)[row as usize].0.contains("DoneWhen"));
    }

    #[test]
    fn link_values_have_link_style_and_component_names_stay_plain() {
        let mut view = sample_view();
        let buffer = draw(view.render(80), 80, 40);
        let range = find_on_screen(&buffer, "2.1~2.3").expect("range value");
        assert!(is_link_look(style_at(&buffer, range)));
        let biz = find_on_screen(&buffer, "세션을 관리한다").expect("title value");
        assert!(is_link_look(style_at(&buffer, biz)));
        let component = find_on_screen(&buffer, "signup::form").expect("component value");
        assert!(!is_link_look(style_at(&buffer, component)));
        let plain_tag = find_on_screen(&buffer, "low").expect("Difficulty value");
        assert!(!is_link_look(style_at(&buffer, plain_tag)));
    }

    #[test]
    fn links_cover_only_shape_matching_values_in_document_order() {
        let mut view = sample_view();
        view.render(80);
        let found: Vec<(&str, &str)> =
            view.links.iter().map(|l| (l.tag_name.as_str(), l.value.as_str())).collect();
        assert_eq!(
            found,
            vec![
                ("Requirements", "1.1"),
                ("Requirements", "1.2"),
                ("BizProcess", "가입 양식을 제출한다"),
                ("Requirements", "1.1"),
                ("Requirements", "2.1~2.3"),
                ("BizProcess", "세션을 관리한다"),
                ("Depends", "1.1"),
                ("Depends", "1.2"),
                ("Requirements", "2.5"),
                ("Depends", "2.1"),
                ("Requirements", "3.1"),
                ("BizProcess", "탈퇴를 처리한다"),
            ]
        );
    }

    #[test]
    fn link_positions_point_at_their_value_in_the_plain_lines() {
        let mut view = sample_view();
        let rendered = view.render(80);
        assert!(!view.links.is_empty());
        for link in &view.links {
            let text = &rendered.plain[link.line];
            let prefix: String = take_width(text, link.column);
            let rest = &text[prefix.len()..];
            assert!(rest.starts_with(&link.value), "{link:?} in {text:?}");
            assert_eq!(link.width, link.value.width());
        }
    }

    fn take_width(text: &str, cells: usize) -> String {
        let mut out = String::new();
        let mut used = 0;
        for c in text.chars() {
            if used >= cells {
                break;
            }
            used += unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            out.push(c);
        }
        out
    }

    #[test]
    fn selecting_a_link_reverses_only_that_value() {
        let mut view = sample_view();
        view.render(80);
        assert_eq!(view.select_next_link(0), Some(0));
        let rendered = view.render(80);
        let first = view.links[0].clone();
        let buffer = draw(rendered, 80, 40);
        let panel_border_rows = 1;
        let screen_row = first.line + panel_border_rows;
        let (text, columns) = rows_with_columns(&buffer)[screen_row].clone();
        let x = columns[text.find(&first.value).map(|b| text[..b].chars().count()).unwrap()];
        assert!(is_selected_look(style_at(&buffer, (x, screen_row as u16))));
        let other = find_on_screen(&buffer, "2.1~2.3").unwrap();
        assert!(!is_selected_look(style_at(&buffer, other)));
        assert!(is_link_look(style_at(&buffer, other)));
    }

    #[test]
    fn selection_moves_forward_and_backward_and_stops_at_the_ends() {
        let mut view = sample_view();
        view.render(80);
        let last = view.links.len() - 1;
        assert_eq!(view.select_previous_link(0), None);
        assert_eq!(view.selected_link, None);
        assert_eq!(view.select_next_link(0), Some(0));
        assert_eq!(view.select_next_link(0), Some(1));
        assert_eq!(view.select_previous_link(0), Some(0));
        assert_eq!(view.select_previous_link(0), None);
        assert_eq!(view.selected_link, Some(0));
        view.selected_link = Some(last);
        assert_eq!(view.select_next_link(0), None);
        assert_eq!(view.selected_link, Some(last));
    }

    #[test]
    fn first_selection_starts_from_the_given_line() {
        let mut view = sample_view();
        view.render(80);
        let target = view.links.iter().position(|l| l.line >= 10).unwrap();
        assert_eq!(view.select_next_link(10), Some(target));
        view.selected_link = None;
        let before = view.links.iter().rposition(|l| l.line < 10).unwrap();
        assert_eq!(view.select_previous_link(10), Some(before));
    }

    #[test]
    fn selection_survives_rerender_at_another_width() {
        let mut view = sample_view();
        view.render(80);
        view.select_next_link(0);
        view.select_next_link(0);
        view.render(30);
        assert_eq!(view.selected_link, Some(1));
    }

    #[test]
    fn narrow_width_wraps_lines_without_splitting_link_values() {
        let mut view = sample_view();
        let rendered = view.render(30);
        for line in &rendered.lines {
            let text: String = line.spans.iter().map(|s| s.text.as_str()).collect();
            assert!(
                usize::from(line.indent) + text.width() <= 30,
                "too wide: {text:?} indent {}",
                line.indent
            );
        }
        let buffer = draw(rendered, 30, 80);
        assert!(find_on_screen(&buffer, "2.1~2.3").is_some(), "range value stays whole");
        assert!(find_on_screen(&buffer, "세션을 관리한다").is_some(), "title value stays whole");
    }

    #[test]
    fn long_subline_wraps_onto_continuation_lines_at_the_same_indent() {
        let mut view = view_of("- [ ] 1 a\n  - _DoneWhen: 하나 둘 셋 넷 다섯 여섯 일곱 여덟 아홉 열_\n");
        let rendered = view.render(24);
        let sublines: Vec<&Line> =
            rendered.lines.iter().filter(|l| l.indent > 0).collect();
        assert!(sublines.len() >= 2);
        assert!(sublines.iter().all(|l| l.indent == sublines[0].indent));
    }

    #[test]
    fn plain_and_lines_stay_parallel() {
        let mut view = sample_view();
        let rendered = view.render(40);
        assert_eq!(rendered.plain.len(), rendered.lines.len());
        for (line, plain) in rendered.lines.iter().zip(&rendered.plain) {
            let text: String = line.spans.iter().map(|s| s.text.as_str()).collect();
            assert_eq!(&text, plain);
        }
    }

    #[test]
    fn major_tasks_and_document_headings_are_registered_as_anchors() {
        let mut view = sample_view();
        let rendered = view.render(80);
        let anchor_lines: Vec<(&str, usize)> =
            rendered.headings.iter().map(|h| (h.text.as_str(), h.line)).collect();
        for number in ["1", "2"] {
            let anchor = rendered
                .headings
                .iter()
                .find(|h| h.text.starts_with(&format!("{number} ")))
                .unwrap_or_else(|| panic!("major task {number} in {anchor_lines:?}"));
            assert!(rendered.plain[anchor.line].contains(number));
        }
        assert!(rendered.headings.iter().all(|h| !h.text.starts_with("1.1")));
        assert!(rendered.headings.iter().any(|h| h.text.contains("구현 계획")));
        let lines: Vec<usize> = rendered.headings.iter().map(|h| h.line).collect();
        assert!(lines.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn document_without_checkboxes_shows_a_notice_line_after_its_text() {
        let mut view = view_of("# 작업 목록\n\n아직 작업이 없다\n");
        let rendered = view.render(60);
        assert!(view.links.is_empty());
        let buffer = draw(rendered, 60, 12);
        let (_, text_row) = find_on_screen(&buffer, "아직 작업이 없다").expect("body text");
        let (_, notice_row) = find_on_screen(&buffer, "작업 항목이 없습니다").expect("notice");
        assert!(notice_row > text_row);
    }

    #[test]
    fn empty_document_renders_only_the_notice() {
        let mut view = view_of("");
        let rendered = view.render(60);
        assert_eq!(rendered.plain.len(), 1);
        assert!(rendered.plain[0].contains("작업 항목이 없습니다"));
    }

    #[test]
    fn document_with_tasks_has_no_notice() {
        let mut view = sample_view();
        let rendered = view.render(80);
        assert!(rendered.plain.iter().all(|l| !l.contains("작업 항목이 없습니다")));
    }

    #[test]
    fn settings_without_link_rule_leave_every_value_plain() {
        let settings = MethodologySettings::parse_overrides(
            r#"{"tags":[{"name":"Requirements","link":null},{"name":"Depends","link":null},{"name":"BizProcess","link":null},{"name":"Boundary","link":null}]}"#,
        )
        .unwrap();
        let mut view = TasksView::build(parse_tasks(SAMPLE, &settings), &settings);
        view.render(80);
        assert!(view.links.is_empty());
    }

    #[test]
    fn mixed_value_list_links_only_the_number_shaped_entries() {
        let mut view = view_of("- [ ] 1 a\n  - _Requirements: 1.1, 설명, 2.1~2.2_\n");
        view.render(80);
        let values: Vec<&str> = view.links.iter().map(|l| l.value.as_str()).collect();
        assert_eq!(values, vec!["1.1", "2.1~2.2"]);
    }

    #[test]
    fn rendering_is_deterministic_and_does_not_panic_on_tiny_widths() {
        let mut view = sample_view();
        for width in [1u16, 2, 5, 10] {
            let first = view.render(width);
            let second = view.render(width);
            assert_eq!(first.plain, second.plain);
        }
    }
}
