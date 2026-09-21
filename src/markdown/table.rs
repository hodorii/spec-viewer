use super::{Line, LineStyle, Span, SpanStyle};
use crate::markdown::wrap::{line_width, wrap_spans};

/// Distribute `available` width across `natural` column widths so the table
/// never exceeds the pane (요구 5.4 — "Table, GitHub style" in design.md):
/// column width is the content width; only when the natural total exceeds
/// `available` do we shrink — widest column first, one cell-width unit at a
/// time down to the minimum of 1 — and cells wrap inside their column.  Only
/// if the total still exceeds `available` after every column has hit its
/// minimum width do we scale all columns proportionally as a last resort.
/// There is no leftover-expansion branch: the table never stretches to fill
/// the pane.
pub(crate) fn distribute_widths(natural: &[usize], available: usize) -> Vec<usize> {
    let ncols = natural.len();
    if ncols == 0 {
        return Vec::new();
    }
    let nat_total: usize = natural.iter().sum();
    if nat_total == 0 {
        // 내용이 모두 빈 표: 폭을 그대로 두고 렌더한다(나눗셈 회피).
        return natural.to_vec();
    }
    if nat_total <= available {
        // 내용 폭이 페인 폭 안에 들어오면 그대로 둔다 — 확장하지 않는다(5.4).
        return natural.to_vec();
    } else {
        // 넘치는 폭: 가장 넓은 열부터 1씩 줄여 최소 폭 1까지 내린다(요구 5.4
        // "가장 넓은 열부터 줄이고 셀은 개행"). 넓은 열이 줄어든 폭을 흡수하므로
        // 자연 폭이 작은 열은 최대한 보존된다.
        let mut widths = natural.to_vec();
        let mut sum = nat_total;
        while sum > available {
            if let Some(i) = (0..ncols).filter(|&i| widths[i] > 1).max_by_key(|&i| widths[i]) {
                widths[i] -= 1;
                sum -= 1;
            } else {
                break;
            }
        }
        // 모든 열이 최소 폭(1)인데도 여전히 넘치면(열이 너무 많은 극단 케이스)
        // 마지막 수단으로 모든 열을 비례 축소한다(요구 5.4). 좁은 폭에서 정수
        // 폭 1 미만은 내려가지 않으므로 사실상 1로 유지되고, `render_table` 의
        // `avail = ...max(ncols)` 덕에 실제로는 이 분기에 도달하지 않는다 —
        // 요구사항 순서를 그대로 담기 위한 방어 분기.
        if sum > available {
            widths = widths
                .iter()
                .map(|&w| (w * available / sum).max(1))
                .collect();
        }
        widths
    }
}

fn cell_width(cell: &[Span]) -> usize {
    cell.iter().map(|s| line_width(&s.text)).sum()
}

/// Render collected table rows (row 0 = header) as a bordered grid: a
/// `┌─┬─┐` top rule, `│ cell │ cell │` rows, a `├─┼─┤` rule after the header
/// **and between every pair of body rows**, and a `└─┴─┘` bottom rule
/// (GitHub style, 요구 5.4). Header cells render bold; borders carry the
/// table's `LineStyle::Table` (theme.table_border). Column widths are the
/// content widths — they shrink widest-column-first only when the total would
/// exceed `width`; cells wrap inside their column; a rendered line never
/// exceeds `width`.
///
/// Ported (algorithm only, retargeted to this crate's own `markdown::{Line,
/// Span, SpanStyle, LineStyle}` types) from mdview's `Table::render` /
/// `column_widths` — see THIRD_PARTY.md.
pub fn render_table(
    rows: &[Vec<Vec<Span>>],
    width: u16,
    out: &mut Vec<Line>,
    plain: &mut Vec<String>,
) {
    let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if ncols == 0 {
        return;
    }

    let mut natural = vec![0usize; ncols];
    for row in rows {
        for (c, cell) in row.iter().enumerate() {
            natural[c] = natural[c].max(cell_width(cell));
        }
    }
    // Border overhead: a leading "│", then each column contributes its own
    // " " + content + " " + "│" (3 chars + its width) -- not just a
    // single-space gap between columns.
    let overhead = 3 * ncols + 1;
    let avail = (width as usize).saturating_sub(overhead).max(ncols);
    let widths = distribute_widths(&natural, avail);

    push_rule(&widths, '┌', '┬', '┐', out, plain);
    for (ri, row) in rows.iter().enumerate() {
        render_row(row, &widths, out, plain, ri == 0);
        if ri + 1 < rows.len() {
            push_rule(&widths, '├', '┼', '┤', out, plain);
        }
    }
    push_rule(&widths, '└', '┴', '┘', out, plain);
}

fn push_rule(
    widths: &[usize],
    l: char,
    m: char,
    r: char,
    out: &mut Vec<Line>,
    plain: &mut Vec<String>,
) {
    let mut s = String::new();
    s.push(l);
    for (i, w) in widths.iter().enumerate() {
        s.push_str(&"─".repeat(w + 2));
        s.push(if i + 1 < widths.len() { m } else { r });
    }
    push_line(out, plain, vec![Span { text: s, style: SpanStyle::Plain }]);
}

fn render_row(
    row: &[Vec<Span>],
    widths: &[usize],
    out: &mut Vec<Line>,
    plain: &mut Vec<String>,
    is_header: bool,
) {
    let n = widths.len();
    let wrapped: Vec<Vec<Vec<Span>>> = (0..n)
        .map(|i| {
            let mut cell = row.get(i).cloned().unwrap_or_default();
            if is_header {
                // 헤더 셀은 굵게(요구 5.4 "헤더는 굵게" — SpanStyle::Bold 를
                // 통해 테마의 굵기 표시로 해석된다).
                for s in &mut cell {
                    s.style = SpanStyle::Bold;
                }
            }
            wrap_spans(&cell, widths[i].max(1) as u16, 0)
        })
        .collect();
    let height = wrapped.iter().map(Vec::len).max().unwrap_or(1).max(1);
    for li in 0..height {
        let mut spans: Vec<Span> = vec![Span { text: "│".to_string(), style: SpanStyle::Plain }];
        for c in 0..n {
            spans.push(Span { text: " ".to_string(), style: SpanStyle::Plain });
            let cell_line: Vec<Span> = wrapped[c].get(li).cloned().unwrap_or_default();
            let used = cell_width(&cell_line);
            let cw = widths[c];
            spans.extend(cell_line);
            if used < cw {
                spans.push(Span {
                    text: " ".repeat(cw - used),
                    style: SpanStyle::Plain,
                });
            }
            spans.push(Span { text: " ".to_string(), style: SpanStyle::Plain });
            spans.push(Span { text: "│".to_string(), style: SpanStyle::Plain });
        }
        push_line(out, plain, spans);
    }
}

fn push_line(out: &mut Vec<Line>, plain: &mut Vec<String>, spans: Vec<Span>) {
    let t: String = spans.iter().map(|s| s.text.as_str()).collect();
    out.push(Line {
        indent: 0,
        spans,
        style: Some(LineStyle::Table),
    });
    plain.push(t);
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    fn cell(s: &str) -> Vec<Span> {
        vec![Span { text: s.to_string(), style: SpanStyle::Plain }]
    }

    fn render(rows: &[Vec<Vec<Span>>], width: u16) -> Vec<String> {
        let mut out = Vec::new();
        let mut plain = Vec::new();
        render_table(rows, width, &mut out, &mut plain);
        plain
    }

    #[test]
    fn short_table_uses_content_width_not_pane_width() {
        // 요구 5.4(개정): 열 폭은 내용 폭 — 페인 폭을 채우지 않는다. 3열 짧은 표
        // (내용 폭 합 2+2+3=7, 테두리 오버헤드 3*3+1=10) 는 avail 80 에서도
        // 표시 폭 17 로 그려야 한다.
        let rows = vec![
            vec![cell("aa"), cell("b"), cell("ccc")],
            vec![cell("d"), cell("ee"), cell("f")],
        ];
        let plain = render(&rows, 80);
        assert!(!plain.is_empty());
        for line in &plain {
            assert_eq!(
                UnicodeWidthStr::width(line.as_str()),
                17,
                "table must use content width (17), not fill the 80-wide pane: {line:?}"
            );
        }
    }

    #[test]
    fn separators_between_header_and_body_rows() {
        // 요구 5.4(개정): `├─┼─┤` 는 헤더 뒤뿐 아니라 모든 본문 행 사이에도
        // 들어간다. 헤더 1 + 본문 3 행 → 구분선은 헤더 뒤 1 개 + 본문 행 사이
        // 2 개 = 총 3 개. 헤더 셀은 굵게(theme.table_header), 테두리·구분선은
        // Plain(LineStyle::Table → theme.table_border).
        let rows = vec![
            vec![cell("h1"), cell("h2")],
            vec![cell("a"), cell("b")],
            vec![cell("c"), cell("d")],
            vec![cell("e"), cell("f")],
        ];
        let mut out = Vec::new();
        let mut plain = Vec::new();
        render_table(&rows, 80, &mut out, &mut plain);
        let seps = plain.iter().filter(|l| l.starts_with('├')).count();
        assert_eq!(
            seps, 3,
            "expected a ├ separator after the header and between each of the 3 body rows (got {seps}):\n{}",
            plain.join("\n")
        );
        let header_line = out
            .iter()
            .find(|l| l.spans.iter().any(|s| s.text.contains("h1")))
            .expect("header row rendered");
        let has_bold_cell = header_line.spans.iter().any(|s| s.style == SpanStyle::Bold);
        assert!(has_bold_cell, "header cells must render bold (theme.table_header)");
    }

    #[test]
    fn wide_table_wraps_within_avail() {
        let rows = vec![
            vec![cell("name"), cell("value"), cell("note")],
            vec![
                cell("alpha"),
                cell("a very long value that must wrap inside its column"),
                cell("x"),
            ],
        ];
        let plain = render(&rows, 40);
        assert!(!plain.is_empty());
        for line in &plain {
            assert!(UnicodeWidthStr::width(line.as_str()) <= 40, "row exceeds avail: {line:?}");
        }
        let data_lines = plain.iter().filter(|l| l.starts_with('│')).count();
        assert!(data_lines > 2, "cells should wrap inside their columns");
    }

    #[test]
    fn tiny_avail_keeps_min_width_without_panic() {
        let rows = vec![
            vec![cell("h1"), cell("h2"), cell("h3")],
            vec![cell("a very long cell that does not fit"), cell("x"), cell("y")],
        ];
        let plain = render(&rows, 3);
        assert!(!plain.is_empty());
        let top = plain.first().expect("table rendered");
        let inner = top.trim_start_matches('┌').trim_end_matches('┐');
        for seg in inner.split('┬') {
            assert!(seg.len() >= 3, "column must keep min width 1: {seg:?}");
        }
    }
}