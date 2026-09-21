use super::{Span, SpanStyle};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

fn grapheme_width(g: &str) -> usize {
    g.chars().map(|c| UnicodeWidthChar::width(c).unwrap_or(0)).sum()
}

pub(crate) fn line_width(s: &str) -> usize {
    s.graphemes(true).map(|g| grapheme_width(g)).sum()
}

fn wrap_token(token: &str, max: usize) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut acc = String::new();
    for g in token.graphemes(true) {
        let w = grapheme_width(g);
        if acc.is_empty() {
            if w <= max {
                acc.push_str(g);
            } else {
                // single grapheme longer than max; emit as its own line
                parts.push(g.to_string());
            }
        } else {
            if line_width(&acc) + w <= max {
                acc.push_str(g);
            } else {
                parts.push(acc);
                acc = String::new();
                if w <= max {
                    acc.push_str(g);
                } else {
                    parts.push(g.to_string());
                }
            }
        }
    }
    if !acc.is_empty() {
        parts.push(acc);
    }
    parts
}

fn push_word(out: &mut Vec<Span>, word: &str, style: SpanStyle, leading_space: bool) {
    if leading_space {
        if let Some(last) = out.last_mut() {
            if last.style == style {
                last.text.push(' ');
                last.text.push_str(word);
                return;
            }
        }
        out.push(Span { text: " ".to_string(), style: SpanStyle::Plain });
        out.push(Span { text: word.to_string(), style });
        return;
    }
    if let Some(last) = out.last_mut() {
        if last.style == style {
            last.text.push_str(word);
            return;
        }
    }
    out.push(Span { text: word.to_string(), style });
}

pub fn wrap_spans(spans: &[Span], width: u16, indent: u16) -> Vec<Vec<Span>> {
    let max = usize::from(width.saturating_sub(indent).max(1));
    let mut words: Vec<(String, SpanStyle)> = Vec::new();
    for s in spans {
        if s.style == SpanStyle::Plain {
            for w in s.text.split_whitespace() {
                words.push((w.to_string(), s.style));
            }
        } else {
            words.push((s.text.clone(), s.style));
        }
    }
    let mut lines: Vec<Vec<Span>> = Vec::new();
    let mut cur: Vec<Span> = Vec::new();
    let mut cur_w: usize = 0;
    for (word, style) in words {
        let w = line_width(&word);
        if w == 0 {
            continue;
        }
        let sep = usize::from(!cur.is_empty());
        if w > max {
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
                cur_w = 0;
            }
            let parts = wrap_token(&word, max);
            for (i, p) in parts.iter().enumerate() {
                let sp = Span { text: p.clone(), style };
                if i == parts.len() - 1 {
                    cur.push(sp);
                    cur_w = line_width(p);
                } else {
                    lines.push(vec![sp]);
                }
            }
        } else if cur_w + sep + w <= max {
            push_word(&mut cur, &word, style, sep == 1);
            cur_w += sep + w;
        } else {
            lines.push(std::mem::take(&mut cur));
            push_word(&mut cur, &word, style, false);
            cur_w = w;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}
