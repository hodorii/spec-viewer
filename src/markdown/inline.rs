use super::{FootnoteState, Span, SpanStyle};
use crate::markdown::theme::{CHECK_DONE, CHECK_TODO};
use pulldown_cmark::{Event, Tag, TagEnd};

fn push(out: &mut Vec<Span>, text: &str, style: SpanStyle) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut() {
        if last.style == style {
            last.text.push_str(text);
            return;
        }
    }
    out.push(Span { text: text.to_string(), style });
}

/// Collect inline events into styled spans until `stop` matches the event at
/// `*idx` (the stop event is not consumed; the caller consumes it).
pub fn collect_inline<F>(events: &[Event], idx: &mut usize, fns: &mut FootnoteState, stop: F) -> Vec<Span>
where
    F: Fn(&Event) -> bool,
{
    let mut spans: Vec<Span> = Vec::new();
    while *idx < events.len() {
        if stop(&events[*idx]) {
            break;
        }
        match &events[*idx] {
            Event::Text(t) => {
                push(&mut spans, t.as_ref(), SpanStyle::Plain);
                *idx += 1;
            }
            Event::Code(c) => {
                push(&mut spans, c.as_ref(), SpanStyle::Code);
                *idx += 1;
            }
            Event::SoftBreak | Event::HardBreak => {
                push(&mut spans, " ", SpanStyle::Plain);
                *idx += 1;
            }
            Event::TaskListMarker(true) => {
                push(&mut spans, &format!("{CHECK_DONE} "), SpanStyle::Plain);
                *idx += 1;
            }
            Event::TaskListMarker(false) => {
                push(&mut spans, &format!("{CHECK_TODO} "), SpanStyle::Plain);
                *idx += 1;
            }
            Event::FootnoteReference(name) => {
                // 10.5: 참조 지점에는 첫 등장 순서로 배정된 번호만 남기고, 본문은
                // mod.rs 의 메인 루프가 FootnoteDefinition 을 모아 문서 끝에 낸다.
                let n = fns.ref_index(name.as_ref());
                push(&mut spans, &format!("[{n}]"), SpanStyle::Plain);
                *idx += 1;
            }
            Event::Start(Tag::Emphasis) => {
                collect_tagged(events, idx, fns, TagEnd::Emphasis, SpanStyle::Italic, &mut spans);
            }
            Event::Start(Tag::Strong) => {
                collect_tagged(events, idx, fns, TagEnd::Strong, SpanStyle::Bold, &mut spans);
            }
            Event::Start(Tag::Strikethrough) => {
                collect_tagged(events, idx, fns, TagEnd::Strikethrough, SpanStyle::Strikethrough, &mut spans);
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                // 10.4: 링크는 `텍스트 (URL)` — 이전에는 URL 을 버리고 숨겼다.
                let url = dest_url.to_string();
                let mut inner_out: Vec<Span> = Vec::new();
                collect_tagged(events, idx, fns, TagEnd::Link, SpanStyle::Link, &mut inner_out);
                spans.extend(inner_out);
                push(&mut spans, &format!(" ({url})"), SpanStyle::Plain);
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                // 10.4: 이미지는 `🖼 대체텍스트 (URL)`.
                let url = dest_url.to_string();
                *idx += 1;
                let mut alt = String::new();
                while *idx < events.len() {
                    match &events[*idx] {
                        Event::End(TagEnd::Image) => {
                            *idx += 1;
                            break;
                        }
                        Event::Text(t) => {
                            alt.push_str(t.as_ref());
                            *idx += 1;
                        }
                        _ => {
                            *idx += 1;
                        }
                    }
                }
                let label = if alt.trim().is_empty() {
                    format!("\u{1F5BC} ({url})")
                } else {
                    format!("\u{1F5BC} {} ({url})", alt.trim())
                };
                push(&mut spans, &label, SpanStyle::Plain);
            }
            _ => {
                *idx += 1;
            }
        }
    }
    spans
}

fn collect_tagged(
    events: &[Event],
    idx: &mut usize,
    fns: &mut FootnoteState,
    end: TagEnd,
    style: SpanStyle,
    out: &mut Vec<Span>,
) {
    *idx += 1;
    let inner = collect_inline(events, idx, fns, |e| {
        matches!(e, Event::End(t) if *t == end)
    });
    if *idx < events.len() {
        *idx += 1;
    }
    for s in inner {
        let st = if s.style == SpanStyle::Plain { style } else { s.style };
        push(out, &s.text, st);
    }
}