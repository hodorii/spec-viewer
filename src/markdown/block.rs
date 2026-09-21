use super::{FootnoteState, HeadingAnchor, Line, LineStyle, Span, SpanStyle};
use crate::markdown::inline::collect_inline;
use crate::markdown::table::render_table;
use crate::markdown::theme::{list_marker_char, Theme, QUOTE_BAR};
use crate::markdown::wrap::wrap_spans;
use crate::markdown::code::push_code_block;
use pulldown_cmark::{Event, Tag, TagEnd};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

fn display_width(s: &str) -> usize {
    s.graphemes(true)
        .map(|g| g.chars().map(|c| UnicodeWidthChar::width(c).unwrap_or(0)).sum::<usize>())
        .sum()
}

pub fn push_heading(
    spans: Vec<Span>,
    level: u8,
    out: &mut Vec<Line>,
    plain: &mut Vec<String>,
    headings: &mut Vec<HeadingAnchor>,
    theme: &Theme,
) {
    let text: String = spans.iter().map(|s| s.text.as_str()).collect();
    let trimmed = text.trim().to_string();
    let line_idx = out.len();
    out.push(Line {
        indent: 0,
        spans,
        style: Some(LineStyle::Heading(level)),
    });
    // `lines` and `plain` must stay parallel (every other push_* function in
    // this module pushes exactly one entry to each per line) — search and
    // scroll logic elsewhere index `plain` and `lines` interchangeably.
    plain.push(trimmed.clone());
    headings.push(HeadingAnchor {
        level,
        text: trimmed.clone(),
        line: line_idx,
    });
    // 10.1: H1/H2 밑줄 — 테마가 그 레벨에 문자를 지정했을 때만, 제목 표시폭만큼.
    if let Some(rule_char) = theme.heading_rule_char(level) {
        let w = display_width(&trimmed);
        if w > 0 {
            let rule: String = std::iter::repeat(rule_char).take(w).collect();
            out.push(Line {
                indent: 0,
                spans: vec![Span { text: rule.clone(), style: SpanStyle::Plain }],
                style: Some(LineStyle::Plain),
            });
            plain.push(rule);
        }
    }
}

pub fn push_paragraph(
    events: &[Event],
    idx: &mut usize,
    out: &mut Vec<Line>,
    plain: &mut Vec<String>,
    width: u16,
    fns: &mut FootnoteState,
) {
    if *idx >= events.len() {
        return;
    }
    if let Event::Start(Tag::Paragraph) = events[*idx] {
        *idx += 1;
        let spans = collect_inline(events, idx, fns, |e| matches!(e, Event::End(TagEnd::Paragraph)));
        if *idx < events.len() {
            *idx += 1;
        }
        let wrapped = wrap_spans(&spans, width, 0);
        for piece in wrapped {
            let t: String = piece.iter().map(|s| s.text.as_str()).collect();
            out.push(Line {
                indent: 0,
                spans: piece,
                style: Some(LineStyle::Plain),
            });
            plain.push(t);
        }
    } else {
        *idx += 1;
    }
}

pub fn push_list(
    events: &[Event],
    idx: &mut usize,
    depth: usize,
    out: &mut Vec<Line>,
    plain: &mut Vec<String>,
    width: u16,
    fns: &mut FootnoteState,
) {
    if *idx >= events.len() {
        return;
    }
    if let Event::Start(Tag::List(_)) = events[*idx] {
        *idx += 1;
        loop {
            if *idx >= events.len() {
                break;
            }
            match &events[*idx] {
                Event::End(TagEnd::List(_)) => {
                    *idx += 1;
                    break;
                }
                Event::Start(Tag::Item) => {
                    *idx += 1;
                    let content = collect_inline(events, idx, fns, |e| {
                        matches!(e, Event::End(TagEnd::Item))
                            || matches!(e, Event::Start(Tag::List(_)))
                    });
                    let nested = *idx < events.len()
                        && matches!(events[*idx], Event::Start(Tag::List(_)));
                    if *idx < events.len() && matches!(events[*idx], Event::End(TagEnd::Item)) {
                        *idx += 1;
                    }
                    let indent = ((depth.saturating_sub(1)) * 2) as u16;
                    let marker = list_marker_char(depth);
                    let mut item_spans = vec![Span {
                        text: format!("{marker} "),
                        style: SpanStyle::Plain,
                    }];
                    item_spans.extend(content);
                    let wrapped = wrap_spans(&item_spans, width, indent);
                    for piece in wrapped {
                        let t: String = piece.iter().map(|s| s.text.as_str()).collect();
                        out.push(Line {
                            indent,
                            spans: piece,
                            style: Some(LineStyle::ListItem(depth as u16)),
                        });
                        plain.push(t);
                    }
                    if nested {
                        push_list(events, idx, depth + 1, out, plain, width, fns);
                    }
                }
                _ => {
                    *idx += 1;
                }
            }
        }
    }
}

pub fn push_table(
    events: &[Event],
    idx: &mut usize,
    out: &mut Vec<Line>,
    plain: &mut Vec<String>,
    width: u16,
    fns: &mut FootnoteState,
) {
    if *idx >= events.len() {
        return;
    }
    if let Event::Start(Tag::Table(_)) = events[*idx] {
        *idx += 1;
        let mut rows: Vec<Vec<Vec<Span>>> = Vec::new();
        loop {
            if *idx >= events.len() {
                break;
            }
            match &events[*idx] {
                Event::End(TagEnd::Table) => {
                    *idx += 1;
                    break;
                }
                Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => {
                    *idx += 1;
                    let mut row: Vec<Vec<Span>> = Vec::new();
                    loop {
                        if *idx >= events.len() {
                            break;
                        }
                        match &events[*idx] {
                            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                                *idx += 1;
                                break;
                            }
                            Event::Start(Tag::TableCell) => {
                                *idx += 1;
                                let cell = collect_inline(events, idx, fns, |e| {
                                    matches!(e, Event::End(TagEnd::TableCell))
                                });
                                if *idx < events.len() {
                                    *idx += 1;
                                }
                                row.push(cell);
                            }
                            _ => {
                                *idx += 1;
                            }
                        }
                    }
                    rows.push(row);
                }
                _ => {
                    *idx += 1;
                }
            }
        }
        render_table(&rows, width, out, plain);
    } else {
        *idx += 1;
    }
}

pub fn push_quote(
    events: &[Event],
    idx: &mut usize,
    out: &mut Vec<Line>,
    plain: &mut Vec<String>,
    width: u16,
    fns: &mut FootnoteState,
) {
    if *idx >= events.len() {
        return;
    }
    if let Event::Start(Tag::BlockQuote(_)) = events[*idx] {
        *idx += 1;
        let content =
            collect_inline(events, idx, fns, |e| matches!(e, Event::End(TagEnd::BlockQuote(_))));
        if *idx < events.len() {
            *idx += 1;
        }
        // 막대(`│ `)는 wrap 대상 스팬에 섞지 않고 매 줄 앞에 별도 스팬으로 붙인다 —
        // `wrap_spans`가 같은 style 의 인접 스팬을 병합하므로, 섞으면 막대와 본문이
        // 하나의 스팬이 되어 테마가 막대만 따로 칠할 수 없다(10.1 quote_bar). 2칸은
        // "│ " 표시폭만큼 wrap 최대폭에서 미리 뺀 것.
        let wrapped = wrap_spans(&content, width, 2);
        for piece in wrapped {
            let mut spans = vec![Span {
                text: format!("{QUOTE_BAR} "),
                style: SpanStyle::Plain,
            }];
            spans.extend(piece);
            let t: String = spans.iter().map(|s| s.text.as_str()).collect();
            out.push(Line {
                indent: 0,
                spans,
                style: Some(LineStyle::Quote),
            });
            plain.push(t);
        }
    } else {
        *idx += 1;
    }
}