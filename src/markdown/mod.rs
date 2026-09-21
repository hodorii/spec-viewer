pub mod block;
pub mod hangul;
pub mod inline;
pub mod table;
pub mod theme;
pub mod wrap;
pub mod code;
pub mod mermaid;

use self::block::{push_heading, push_list, push_paragraph, push_quote, push_table};
use self::inline::collect_inline;
use self::code::push_code_block;
pub use self::theme::Theme;


#[derive(Clone)]
pub struct Line {
    pub indent: u16,
    pub spans: Vec<Span>,
    pub style: Option<LineStyle>, // heading, quote, list, etc.
}

#[derive(Clone)]
pub struct Span {
    pub text: String,
    pub style: SpanStyle,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SpanStyle { Plain, Bold, Italic, Strikethrough, Code, Link }

#[derive(Clone, Copy)]
pub enum LineStyle { Heading(u8), Quote, ListItem(u16), Table, Plain }

pub struct HeadingAnchor { pub level: u8, pub text: String, pub line: usize }

/// 각주 하나 — 문서 끝에 모아 출력한다(요구사항 10.5). `index` 는 첫 참조 순서(1-based).
pub struct Footnote { pub index: usize, pub name: String, pub text: String }

pub struct Rendered {
    pub lines: Vec<Line>,
    pub plain: Vec<String>,
    pub headings: Vec<HeadingAnchor>,
    pub footnotes: Vec<Footnote>,
}

/// 각주 참조·정의를 모으는 렌더 중 상태. `collect_inline` 이 `FootnoteReference` 를
/// 만나면 이걸로 인덱스를 받아 `[n]` 을 인라인에 남기고, 정의 본문은 별도로 쌓아
/// 문서 끝에 번호와 함께 낸다.
#[derive(Default)]
pub struct FootnoteState {
    defs: std::collections::HashMap<String, String>,
    order: Vec<String>,
}

impl FootnoteState {
    /// 이름에 대응하는 1-based 인덱스. 처음 보는 이름이면 등장 순서대로 새로 배정한다.
    pub(crate) fn ref_index(&mut self, name: &str) -> usize {
        if let Some(pos) = self.order.iter().position(|n| n == name) {
            pos + 1
        } else {
            self.order.push(name.to_string());
            self.order.len()
        }
    }

    pub(crate) fn define(&mut self, name: &str, text: String) {
        self.defs.insert(name.to_string(), text);
    }
}

fn strip_frontmatter(src: &str) -> &str {
    if !src.starts_with("---\n") {
        return src;
    }
    let body = &src[4..];
    let Some(pos) = body.find("\n---") else {
        return src;
    };
    let after = pos + 4;
    if after > body.len() {
        return src;
    }
    body[after..].strip_prefix('\n').unwrap_or(&body[after..])
}

/// Render markdown to a wrapped, line-oriented representation, using the
/// default `Theme` (task 10.1: signature preserved for existing call sites —
/// see `render_with` for the themed entry point).
pub fn render(src: &str, width: u16) -> Rendered {
    render_with(src, width, &Theme::default())
}

/// Same as `render`, but takes an explicit `Theme` — currently only
/// `Theme.heading_rule` affects structural output (whether/which underline
/// character follows H1~H6); colors are resolved separately via
/// `Theme::span_style`/`Theme::line_style` at paint time (see `theme.rs` tests).
pub fn render_with(src: &str, width: u16, theme: &Theme) -> Rendered {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

    let mut lines: Vec<Line> = Vec::new();
    let mut plain: Vec<String> = Vec::new();
    let mut headings: Vec<HeadingAnchor> = Vec::new();
    let mut fns = FootnoteState::default();

    // 10.6: NFD 자소 분리 입력(주로 macOS 파일)을 파싱 전에 음절로 합친다.
    let composed = hangul::compose(src);
    let src = strip_frontmatter(&composed);
    let options = Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES;
    let parser = Parser::new_ext(src, options);
    let events: Vec<Event> = parser.collect();
    let mut idx: usize = 0;
    while idx < events.len() {
        match &events[idx] {
            Event::Start(Tag::Heading { level, ..}) => {
                let lvl = *level as u8;
                idx += 1;
                let spans = collect_inline(&events, &mut idx, &mut fns, |e| {
                    matches!(e, Event::End(TagEnd::Heading(_)))
                });
                if idx < events.len() {
                    idx += 1;
                }
                push_heading(spans, lvl, &mut lines, &mut plain, &mut headings, theme);
            }
            Event::Start(Tag::CodeBlock(lang)) => {
                idx += 1;
                let mut code = String::new();
                while idx < events.len() {
                    match &events[idx] {
                        Event::End(TagEnd::CodeBlock) => {
                            idx += 1;
                            break;
                        }
                        Event::Text(t) => {
                            code.push_str(t);
                        }
                        _ => {}
                    }
                    idx += 1;
                }
                let lang_str = match lang {
                    pulldown_cmark::CodeBlockKind::Fenced(s) => s.as_ref(),
                    pulldown_cmark::CodeBlockKind::Indented => "text",
                };
                push_code_block(lang_str, &code, width, &mut lines, &mut plain);
            }
            Event::Start(Tag::Paragraph) => {
                push_paragraph(&events, &mut idx, &mut lines, &mut plain, width, &mut fns);
            }
            Event::Start(Tag::List(_)) => {
                push_list(&events, &mut idx, 1, &mut lines, &mut plain, width, &mut fns);
            }
            Event::Start(Tag::BlockQuote(_)) => {
                push_quote(&events, &mut idx, &mut lines, &mut plain, width, &mut fns);
            }
            Event::Start(Tag::Table(_)) => {
                push_table(&events, &mut idx, &mut lines, &mut plain, width, &mut fns);
            }
            Event::Start(Tag::FootnoteDefinition(name)) => {
                // 정의는 본문 위치에 내지 않고 모았다가 문서 끝에 번호로 낸다(10.5).
                let name = name.to_string();
                idx += 1;
                let mut text = String::new();
                while idx < events.len() {
                    match &events[idx] {
                        Event::End(TagEnd::FootnoteDefinition) => {
                            idx += 1;
                            break;
                        }
                        Event::Text(t) => text.push_str(t),
                        Event::SoftBreak | Event::HardBreak => text.push(' '),
                        _ => {}
                    }
                    idx += 1;
                }
                fns.define(&name, text.trim().to_string());
            }
            Event::Rule => {
                lines.push(Line { indent: 0, spans: vec![Span { text: "---".to_string(), style: SpanStyle::Plain }], style: Some(LineStyle::Plain) });
                plain.push("---".to_string());
                idx += 1;
            }
            _ => { idx += 1; }
        }
    }

    // 각주 — 첫 참조 순서대로 문서 끝에 번호와 함께(10.5).
    let mut footnotes: Vec<Footnote> = Vec::new();
    for (i, name) in fns.order.iter().enumerate() {
        let index = i + 1;
        let text = fns.defs.get(name).cloned().unwrap_or_default();
        footnotes.push(Footnote { index, name: name.clone(), text: text.clone() });
    }
    if !footnotes.is_empty() {
        lines.push(Line { indent: 0, spans: vec![Span { text: "각주".to_string(), style: SpanStyle::Plain }], style: Some(LineStyle::Plain) });
        plain.push("각주".to_string());
        for fnote in &footnotes {
            let entry = format!("[{}] {}", fnote.index, fnote.text);
            for piece in wrap::wrap_spans(&[Span { text: entry.clone(), style: SpanStyle::Plain }], width, 0) {
                let t: String = piece.iter().map(|s| s.text.as_str()).collect();
                lines.push(Line { indent: 0, spans: piece, style: Some(LineStyle::Plain) });
                plain.push(t);
            }
        }
    }

    Rendered { lines, plain, headings, footnotes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthChar;

    fn line_total_width(line: &Line) -> usize {
        let text: String = line.spans.iter().map(|s| s.text.as_str()).collect();
        let w: usize = text
            .graphemes(true)
            .map(|g| g.chars().map(|c| UnicodeWidthChar::width(c).unwrap_or(0) as usize).sum::<usize>())
            .sum();
        w + line.indent as usize
    }

    #[test]
    fn test_paragraph_mixed_width() {
        let md = "안녕 Hello 세계 this is a test paragraph to wrap.";
        let w = 40u16;
        let rendered = render(md, w);
        for line in rendered.lines.iter() {
            assert!(line_total_width(line) <= w as usize, "paragraph line too wide");
        }
    }

    #[test]
    fn test_cjk_wrap_no_split() {
        let text = "안녕하세요안녕하세요안녕하세요안녕하세요안녕하세요";
        let rendered = render(text, 40);
        for line in rendered.lines.iter() {
            let t: String = line.spans.iter().map(|s| s.text.as_str()).collect();
            assert!(!t.contains('-'));
        }
    }

    #[test]
    fn test_nested_list_indents() {
        let md = "- A\n  - B\n    - C";
        let rendered = render(md, 40);
        let mut indents = Vec::new();
        for line in rendered.lines.iter() {
            if let Some(LineStyle::ListItem(_)) = line.style { indents.push(line.indent); }
        }
        assert_eq!(indents, vec![0, 2, 4]);
        for line in rendered.lines.iter() {
            assert!(line_total_width(line) <= 40, "list line too wide");
        }
    }

    #[test]
    fn test_headings_anchors_and_text() {
        let md = "## Title\nBody text\n\n### Subheading";
        let rendered = render(md, 40);
        assert_eq!(rendered.headings.len(), 2);
        assert_eq!(rendered.headings[0].level, 2);
        assert_eq!(rendered.headings[0].text, "Title");
        assert_eq!(rendered.headings[1].level, 3);
        assert_eq!(rendered.headings[1].text, "Subheading");
    }

    #[test]
    fn test_blockquote() {
        let md = "> quoted text";
        let rendered = render(md, 40);
        let has_quote = rendered
            .lines
            .iter()
            .any(|l| matches!(l.style, Some(LineStyle::Quote)));
        assert!(has_quote, "expected a quote line");
        for line in rendered.lines.iter() {
            assert!(line_total_width(line) <= 40, "quote line too wide");
        }
    }

    fn all_text(rendered: &Rendered) -> String {
        rendered
            .lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn test_heading_level_styles() {
        let rendered = render("# H1\n## H2\n### H3", 40);
        let levels: Vec<u8> = rendered
            .lines
            .iter()
            .filter_map(|l| match l.style {
                Some(LineStyle::Heading(lv)) => Some(lv),
                _ => None,
            })
            .collect();
        assert_eq!(levels, vec![1, 2, 3]);
    }

    #[test]
    fn test_checkbox_symbols() {
        // 10.2 (요구사항 10.2): 체크박스는 ✓(완료)/☐(미완료) — 이전 ☑ 에서 교체.
        let rendered = render("- [x] done\n- [ ] todo", 40);
        let text = all_text(&rendered);
        assert!(text.contains('\u{2713}'), "checked box glyph (✓) missing in: {text}");
        assert!(text.contains('☐'), "unchecked box glyph missing in: {text}");
    }

    #[test]
    fn test_yaml_frontmatter_stripped() {
        let md = "---\ntitle: 설정값\npriority: high\n---\n# Title\nbody";
        let rendered = render(md, 40);
        assert_eq!(rendered.headings.len(), 1, "expected only the real heading");
        assert_eq!(rendered.headings[0].text, "Title");
        let all = all_text(&rendered);
        assert!(!all.contains("title:") && !all.contains("priority:"), "frontmatter leaked: {all}");
    }

    #[test]
    fn test_link_text_and_url() {
        // 10.4: 링크는 `텍스트 (URL)` 형식 — 이전 5.10/5.11(URL 숨김)을 대체한다.
        let rendered = render("visit [openai](https://openai.com) now", 40);
        let all = all_text(&rendered);
        assert!(all.contains("(https://openai.com)"), "link URL missing in: {all}");
        let link_spans: Vec<&Span> = rendered
            .lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| s.style == SpanStyle::Link)
            .collect();
        assert!(link_spans.iter().any(|s| s.text == "openai"), "link text not styled Link");
    }

    #[test]
    fn test_image_alt_and_url() {
        // 10.4: 이미지는 `🖼 대체텍스트 (URL)` 형식.
        let rendered = render("text ![diagram](./a.png) end", 40);
        let all = all_text(&rendered);
        assert!(all.contains("\u{1F5BC} diagram (./a.png)"), "image label missing in: {all}");
    }

    #[test]
    fn test_emphasis_styles() {
        let rendered = render("**bold** *italic* ~~strike~~ `code`", 40);
        let spans: Vec<&Span> = rendered.lines.iter().flat_map(|l| l.spans.iter()).collect();
        assert!(spans.iter().any(|s| s.style == SpanStyle::Bold && s.text.contains("bold")));
        assert!(spans.iter().any(|s| s.style == SpanStyle::Italic && s.text.contains("italic")));
        assert!(spans.iter().any(|s| s.style == SpanStyle::Strikethrough && s.text.contains("strike")));
        assert!(spans.iter().any(|s| s.style == SpanStyle::Code && s.text.contains("code")));
    }

    #[test]
    fn test_table_five_cols() {
        let md = "| A | B | C | D | E |\n| --- | --- | --- | --- | --- |\n| 1 | 2 | 3 | 4 | 5 |\n| aaaa | bbbb | cccc | dddd | eeee |";
        for w in [40u16, 80u16] {
            let rendered = render(md, w);
            let has_table = rendered
                .lines
                .iter()
                .any(|l| matches!(l.style, Some(LineStyle::Table)));
            assert!(has_table, "expected table lines at width {w}");
            for line in rendered.lines.iter() {
                assert!(
                    line_total_width(line) <= w as usize,
                    "table line too wide at width {w}"
                );
            }
            let all = all_text(&rendered);
            for needle in ["A", "E", "aaaa", "eeee"] {
                assert!(all.contains(needle), "cell content lost at width {w}: {all}");
            }
        }
    }

    fn is_subsequence(joined_preserving_order: &str, needle: &str) -> bool {
        let mut it = joined_preserving_order.chars();
        needle.chars().all(|n| it.find(|&h| h == n).is_some())
    }

    #[test]
    fn test_table_cell_wrap() {
        let md = "| 헤더 | 내용 |\n| --- | --- |\n| 짧음 | 정말정말긴설명텍스트입니다정말정말긴설명텍스트입니다정말 |";
        let w = 40u16;
        let rendered = render(md, w);
        for line in rendered.lines.iter() {
            assert!(
                line_total_width(line) <= w as usize,
                "table line too wide: {:?}",
                line_total_width(line)
            );
        }
        let all = all_text(&rendered);
        // Cell content is preserved across line breaks (wrapping splits tokens).
        let ordered: String = all.chars().filter(|c| !c.is_whitespace()).collect();
        for needle in ["헤더", "내용", "짧음", "정말정말긴설명텍스트입니다정말정말긴설명텍스트입니다정말"] {
            assert!(
                is_subsequence(&ordered, needle),
                "cell text lost after wrap: {all}"
            );
        }
    }

    #[test]
    fn test_footnote_reference_and_collected_at_end() {
        // 10.5: 참조 지점엔 번호만, 정의는 문서 끝에 모아서.
        let md = "본문 참조[^a] 그리고 더[^b].\n\n[^a]: 첫 각주 내용\n[^b]: 둘째 각주 내용";
        let rendered = render(md, 40);
        assert_eq!(rendered.footnotes.len(), 2);
        assert_eq!(rendered.footnotes[0].index, 1);
        assert_eq!(rendered.footnotes[0].text, "첫 각주 내용");
        assert_eq!(rendered.footnotes[1].index, 2);
        assert_eq!(rendered.footnotes[1].text, "둘째 각주 내용");

        let all = all_text(&rendered);
        assert!(all.contains("참조[1]"), "reference marker [1] missing in: {all}");
        assert!(all.contains("더[2]"), "reference marker [2] missing in: {all}");
        assert!(all.contains("[1] 첫 각주 내용"), "footnote body [1] missing at document end in: {all}");
        assert!(all.contains("[2] 둘째 각주 내용"), "footnote body [2] missing at document end in: {all}");

        // 정의 위치(문서 앞쪽)가 아니라 문서 끝에만 나타나야 한다 — 정의를 제 위치에
        // 그대로 두면 본문에 2번 나오게 된다.
        let occurrences = rendered
            .plain
            .iter()
            .filter(|l| l.contains("첫 각주 내용"))
            .count();
        assert_eq!(occurrences, 1, "footnote body must appear exactly once, at the end");
    }

    #[test]
    fn test_no_footnotes_means_no_footnote_section() {
        let rendered = render("plain text, no footnotes here", 40);
        assert!(rendered.footnotes.is_empty());
        assert!(!all_text(&rendered).contains("각주"));
    }

    #[test]
    fn test_nfd_input_composes_to_nfc_end_to_end() {
        // 10.6: NFD 자소분리 입력(예: macOS 파일에서 온 본문)이 render() 를 통해
        // 정상 음절로 합쳐져 나오는지 — hangul.rs 유닛 테스트가 아니라 렌더 경로
        // 전체를 통과시켜 확인한다.
        let nfd = "\u{1112}\u{1161}\u{11ab}\u{1100}\u{1173}\u{11af} \u{1112}\u{1167}\u{11ab}\u{1109}\u{1161}\u{11bc}";
        let rendered = render(nfd, 40);
        let all = all_text(&rendered);
        assert!(all.contains("한글"), "NFD input did not compose to 한글 in: {all:?}");
        assert!(all.contains("현상"), "NFD input did not compose to 현상 in: {all:?}");
        assert!(!all.chars().any(|c| ('\u{1100}'..='\u{11FF}').contains(&c)), "leftover decomposed jamo in: {all:?}");
    }

    #[test]
    fn test_korean_paragraph_word_wrap_width_40() {
        // 10.6: 어절(공백) 단위 줄바꿈 — 폭 40 한글 문단에서 단어 중간이 끊기지
        // 않고, 각 줄이 폭을 넘지 않는지 확인한다("골든" 성격의 대표 문단 픽스처).
        let md = "이 문단은 한글 어절 단위 줄바꿈이 폭 40에서도 단어 중간을 끊지 않고 \
                  공백 경계에서만 줄이 바뀌는지 확인하기 위한 대표 픽스처 문장입니다.";
        let rendered = render(md, 40);
        for line in &rendered.lines {
            assert!(line_total_width(line) <= 40, "line exceeds width 40");
        }
        // 재구성한 전체 텍스트에서 원문 어절이 온전히 보존되는지(중간에 끊겨 다른
        // 글자가 끼어들지 않는지) 확인한다.
        let all = all_text(&rendered);
        for word in ["이 문단은", "한글 어절", "확인하기", "픽스처 문장입니다."] {
            assert!(
                all.split_whitespace().collect::<Vec<_>>().join(" ").contains(word),
                "word boundary broken near {word:?} in: {all:?}"
            );
        }
    }
}
