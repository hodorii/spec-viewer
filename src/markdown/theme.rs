//! 렌더링 테마 — 헤딩·강조·인용 등 색상 스타일의 유일한 원천(task 10.1).
//!
//! 구조는 `~/dev/tools/mdview`(MIT)의 `src/theme.rs`를 참고했다 — 필드 이름과 계층
//! (heading[6] / heading_rule[6] / emphasis / strong / ... / table_border)을 그대로
//! 따랐다. 라이선스 고지는 `spec-viewer/THIRD_PARTY.md` 참조.
//!
//! 목록 기호(`•` `◦` `▪`)와 체크박스(`✓` `□`)는 요구사항 10.2 가 고정 문자로 못박은
//! 값이라 테마 데이터가 아니라 `block.rs`/`inline.rs` 의 상수로 둔다 — 테마가 바뀌어도
//! 바뀌지 않는다. `heading_rule` 만 `Option<char>` 인 이유는 레벨별로 밑줄을 그릴지
//! 말지(H3~H6 는 `None`) 자체가 테마 판단이기 때문이다.

use super::{LineStyle, SpanStyle};
use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Debug)]
pub struct Theme {
    pub heading: [Style; 6],
    /// 제목 아래에 그을 줄 문자. `None` 이면 긋지 않는다(요구사항 10.1: H1 `━`/H2 `─`, H3~H6 없음).
    pub heading_rule: [Option<char>; 6],
    pub emphasis: Style,
    pub strong: Style,
    pub strikethrough: Style,
    pub code: Style,
    pub quote: Style,
    pub quote_bar: Style,
    pub list_marker: Style,
    pub task_done: Style,
    pub task_todo: Style,
    pub rule: Style,
    pub table_header: Style,
    pub table_border: Style,
    pub text: Style,
    pub link: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            text: Style::default().fg(Color::Indexed(252)),
            // H1/H2 는 밑줄(heading_rule)로 구분되므로 같은 색이어도 된다. H3~H6 는
            // 요구사항 10.1이 "색·굵기로 구분"하라고 못박아 넷 모두 서로 달라야 한다
            // (리뷰 지적 — 초판은 H4=H5=H6 이 전부 같은 색이라 구분되지 않았다).
            heading: [
                Style::default().fg(Color::Indexed(110)).add_modifier(Modifier::BOLD),
                Style::default().fg(Color::Indexed(110)).add_modifier(Modifier::BOLD),
                Style::default().fg(Color::Indexed(109)).add_modifier(Modifier::BOLD),
                Style::default().fg(Color::Indexed(252)).add_modifier(Modifier::BOLD),
                Style::default().fg(Color::Indexed(248)),
                Style::default().fg(Color::Indexed(244)),
            ],
            heading_rule: [Some('━'), Some('─'), None, None, None, None],
            emphasis: Style::default().add_modifier(Modifier::ITALIC),
            strong: Style::default().add_modifier(Modifier::BOLD),
            strikethrough: Style::default()
                .add_modifier(Modifier::CROSSED_OUT)
                .fg(Color::Indexed(245)),
            code: Style::default().fg(Color::Indexed(110)).bg(Color::Indexed(236)),
            quote: Style::default().fg(Color::Indexed(245)).add_modifier(Modifier::ITALIC),
            quote_bar: Style::default().fg(Color::Indexed(240)),
            list_marker: Style::default().fg(Color::Indexed(67)),
            task_done: Style::default().fg(Color::Indexed(110)),
            task_todo: Style::default().fg(Color::Indexed(244)),
            rule: Style::default().fg(Color::Indexed(240)),
            table_header: Style::default().fg(Color::Indexed(110)).add_modifier(Modifier::BOLD),
            table_border: Style::default().fg(Color::Indexed(240)),
            link: Style::default().fg(Color::Indexed(110)).add_modifier(Modifier::UNDERLINED),
        }
    }
}

impl Theme {
    /// 레벨(1-based, 1~6)에 대응하는 밑줄 문자. 범위 밖이거나 `None` 이면 긋지 않는다.
    pub fn heading_rule_char(&self, level: u8) -> Option<char> {
        self.heading_rule
            .get((level as usize).saturating_sub(1))
            .copied()
            .flatten()
    }

    /// `SpanStyle` 하나를 실제 렌더 스타일로 해석한다 — 인라인 강조의 유일한 색 원천.
    pub fn span_style(&self, style: SpanStyle) -> Style {
        match style {
            SpanStyle::Plain => self.text,
            SpanStyle::Bold => self.strong,
            SpanStyle::Italic => self.emphasis,
            SpanStyle::Strikethrough => self.strikethrough,
            SpanStyle::Code => self.code,
            SpanStyle::Link => self.link,
        }
    }

    /// 인용의 좌측 막대(`│`) 전용 스타일 — 본문(`quote`)과 의도적으로 분리해 둔다.
    /// 막대는 `push_quote`(block.rs)가 매 줄 별도 스팬으로 만들어 두므로, 이 스타일을
    /// 그 첫 스팬에만 입히면 막대와 본문이 시각적으로 구분된다(10.1 리뷰 지적 해소).
    pub fn quote_bar_style(&self) -> Style {
        self.quote_bar
    }

    /// `LineStyle` 하나를 실제 렌더 스타일로 해석한다 — 블록의 유일한 색 원천.
    pub fn line_style(&self, style: &LineStyle) -> Style {
        match style {
            LineStyle::Heading(level) => self
                .heading
                .get((*level as usize).saturating_sub(1))
                .copied()
                .unwrap_or(self.text),
            LineStyle::Quote => self.quote,
            LineStyle::ListItem(_) => self.text,
            LineStyle::Table => self.table_border,
            LineStyle::Plain => self.text,
        }
    }
}

/// 목록 기호 — 깊이(1-based)별 고정 문자(요구사항 10.2). 3단계 이상은 `▪` 반복.
pub const LIST_MARKERS: [char; 3] = ['•', '◦', '▪'];

pub fn list_marker_char(depth: usize) -> char {
    let idx = depth.saturating_sub(1).min(LIST_MARKERS.len() - 1);
    LIST_MARKERS[idx]
}

/// 체크박스 — 완료/미완료 고정 문자(요구사항 10.2). 미완료는 원래 U+2610
/// BALLOT BOX(☐)였는데, 흔한 CJK 모노스페이스 폰트(기본 Noto Sans Mono
/// CJK 등)의 커버리지 밖이라 터미널이 그 문자에만 다른 폰트로 폴백하는
/// 비용이 있었다(실측: 체크박스가 있는 줄 스크롤이 눈에 띄게 느려짐) —
/// 같은 값을 내지만 훨씬 폭넓게 커버되는 U+25A1 WHITE SQUARE(□)로 교체.
pub const CHECK_DONE: char = '\u{2713}'; // ✓
pub const CHECK_TODO: char = '\u{25A1}'; // □ (이전: ☐ U+2610)

/// 인용 좌측 막대(요구사항 10.3).
pub const QUOTE_BAR: char = '\u{2502}'; // │

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::{render_with, Line, LineStyle, Span, SpanStyle};
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::style::Color;
    use ratatui::text::{Line as RLine, Span as RSpan};
    use ratatui::widgets::Paragraph;
    use ratatui::Terminal;

    /// `markdown::Line`/`Span`(구조 표현)을 테마로 물들여 실제 ratatui 프레임 버퍼로
    /// 그린다. `ui/` 를 거치지 않는, Renderer 경계 내부의 독립 검증 경로다 — task 10.1
    /// DONE 기준("프레임 버퍼 단정")을 `ui/` 호출부를 바꾸지 않고 증명하기 위함.
    fn paint(lines: &[Line], theme: &Theme, width: u16, height: u16) -> ratatui::buffer::Buffer {
        // 스팬 스타일을 바탕(base)으로, 라인 스타일을 그 위에 patch — 헤딩·인용처럼
        // 줄 전체에 걸리는 색이 개별 스팬의(대개 Plain) 색을 이긴다. ui/doc_panel.rs
        // 의 기존 결합 방식(`span_style(..).patch(line_style(..))`)과 동일하게 맞췄다.
        let rlines: Vec<RLine<'static>> = lines
            .iter()
            .map(|l| {
                let base_line_style = l.style.as_ref().map(|s| theme.line_style(s)).unwrap_or(theme.text);
                let is_quote = matches!(l.style, Some(LineStyle::Quote));
                let spans: Vec<RSpan<'static>> = l
                    .spans
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        // 인용 줄의 첫 스팬(│ 막대, block.rs::push_quote 가 항상 별도로
                        // 붙여 둔다)은 본문과 다른 quote_bar 스타일을 쓴다.
                        let style = if is_quote && i == 0 {
                            theme.quote_bar_style()
                        } else {
                            theme.span_style(s.style).patch(base_line_style)
                        };
                        RSpan::styled(s.text.clone(), style)
                    })
                    .collect();
                RLine::from(spans)
            })
            .collect();
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, width, height);
                f.render_widget(Paragraph::new(rlines), area);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn test_heading_rule_lines_present() {
        let theme = Theme::default();
        let rendered = render_with("# H1\n## H2\n### H3", 40, &theme);
        // H1 다음 줄은 ━ 로 채워진 밑줄, H2 다음 줄은 ─, H3 다음에는 밑줄이 없어야 한다.
        let texts: Vec<String> = rendered
            .lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect())
            .collect();
        let h1_idx = rendered.headings.iter().find(|h| h.level == 1).unwrap().line;
        let h2_idx = rendered.headings.iter().find(|h| h.level == 2).unwrap().line;
        let h3_idx = rendered.headings.iter().find(|h| h.level == 3).unwrap().line;
        assert!(texts[h1_idx + 1].chars().all(|c| c == '━') && !texts[h1_idx + 1].is_empty());
        assert!(texts[h2_idx + 1].chars().all(|c| c == '─') && !texts[h2_idx + 1].is_empty());
        // H3 은 문서 마지막 헤딩이므로 그 다음 줄이 없거나(범위 밖), 있어도 ─/━ 로 채워진 줄이 아니어야 한다.
        if h3_idx + 1 < texts.len() {
            assert!(!texts[h3_idx + 1].chars().all(|c| c == '━' || c == '─'));
        }
    }

    #[test]
    fn test_heading_rule_matches_text_width() {
        let theme = Theme::default();
        let rendered = render_with("# 안녕하세요", 40, &theme);
        let h_idx = rendered.headings[0].line;
        let heading_text: String = rendered.lines[h_idx]
            .spans
            .iter()
            .map(|s| s.text.as_str())
            .collect();
        let rule_text: String = rendered.lines[h_idx + 1]
            .spans
            .iter()
            .map(|s| s.text.as_str())
            .collect();
        use unicode_segmentation::UnicodeSegmentation;
        use unicode_width::UnicodeWidthChar;
        let disp_w: usize = heading_text
            .graphemes(true)
            .map(|g| g.chars().map(|c| UnicodeWidthChar::width(c).unwrap_or(0)).sum::<usize>())
            .sum();
        let rule_w: usize = rule_text.chars().count(); // ━/─ 는 폭 1
        assert_eq!(rule_w, disp_w, "heading rule width should match heading display width");
    }

    #[test]
    fn test_frame_buffer_heading_colors_distinguish_levels() {
        // 10.1 리뷰 지적: H3~H6 는 전부 서로 달라야 한다 — H1/H2 만 밑줄로 구분되고
        // 색이 같아도 된다. H4 vs H5, H5 vs H6 를 반드시 비교한다(초판은 이 둘이
        // 같아서 구분이 안 됐다).
        let theme = Theme::default();
        let rendered = render_with("# H1\n### H3\n#### H4\n##### H5\n###### H6", 40, &theme);
        let buf = paint(&rendered.lines, &theme, 40, rendered.lines.len() as u16);
        let style_at = |level: u8| -> Style {
            let line = rendered.headings.iter().find(|h| h.level == level).unwrap().line as u16;
            buf.cell((0, line)).unwrap().style()
        };
        let (h1, h3, h4, h5, h6) = (style_at(1), style_at(3), style_at(4), style_at(5), style_at(6));
        assert_eq!(h1.fg, Some(Color::Indexed(110)));
        assert_eq!(h3.fg, Some(Color::Indexed(109)));
        assert_eq!(h4.fg, Some(Color::Indexed(252)));
        assert_eq!(h5.fg, Some(Color::Indexed(248)));
        assert_eq!(h6.fg, Some(Color::Indexed(244)));
        assert_ne!(h1, h3, "H1/H3 must be visually distinguishable");
        assert_ne!(h3, h4, "H3/H4 must be visually distinguishable");
        assert_ne!(h4, h5, "H4/H5 must be visually distinguishable");
        assert_ne!(h5, h6, "H5/H6 must be visually distinguishable");
    }

    #[test]
    fn test_frame_buffer_quote_bar_present() {
        // 10.1 리뷰 지적: 문자열 검사가 아니라 실제 ratatui 프레임 버퍼에서 막대
        // 셀과 본문 셀의 스타일이 서로 다른지 확인한다.
        let theme = Theme::default();
        let rendered = render_with("> quoted text", 40, &theme);
        let text: String = rendered.lines[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert!(text.starts_with(QUOTE_BAR), "quote should start with the │ bar: {text}");
        let buf = paint(&rendered.lines, &theme, 40, 1);
        assert_eq!(buf.cell((0, 0)).unwrap().symbol(), QUOTE_BAR.to_string());
        let bar_style = buf.cell((0, 0)).unwrap().style();
        // 막대 다음 칸(공백 이후 본문 시작 지점 근처)의 스타일과 달라야 한다.
        let body_style = buf.cell((3, 0)).unwrap().style();
        assert_eq!(bar_style.fg, Some(Color::Indexed(240)), "quote_bar color");
        assert_ne!(bar_style, body_style, "quote bar must be styled differently from quote body");
    }

    #[test]
    fn test_span_style_distinguishes_emphasis() {
        let theme = Theme::default();
        assert_ne!(theme.span_style(SpanStyle::Bold), theme.span_style(SpanStyle::Italic));
        assert_ne!(theme.span_style(SpanStyle::Italic), theme.span_style(SpanStyle::Strikethrough));
        assert_ne!(theme.span_style(SpanStyle::Code), theme.span_style(SpanStyle::Plain));
    }

    #[test]
    fn test_list_marker_chars_by_depth() {
        assert_eq!(list_marker_char(1), '•');
        assert_eq!(list_marker_char(2), '◦');
        assert_eq!(list_marker_char(3), '▪');
        assert_eq!(list_marker_char(4), '▪');
    }
}
