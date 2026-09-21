use super::parse::{Diagram, Message};
use super::Fallback;
use crate::markdown::wrap::line_width;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

const GAP: usize = 2;
const MIN_BUDGET: usize = 3;

fn grapheme_w(g: &str) -> usize {
    g.chars().map(|c| UnicodeWidthChar::width(c).unwrap_or(0)).sum()
}

fn abbreviate(s: &str, max_w: usize) -> String {
    if line_width(s) <= max_w {
        return s.to_string();
    }
    if max_w == 0 {
        return String::new();
    }
    if max_w == 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut w = 0usize;
    for g in s.graphemes(true) {
        let gw = grapheme_w(g);
        if w + gw > max_w - 1 {
            break;
        }
        out.push_str(g);
        w += gw;
    }
    format!("{}…", out)
}

fn trunc(s: &str, budget: Option<usize>) -> String {
    match budget {
        Some(b) => abbreviate(s, b),
        None => s.to_string(),
    }
}

fn pad_center(s: &str, w: usize) -> String {
    let sw = line_width(s);
    if sw >= w {
        return s.to_string();
    }
    let total = w - sw;
    let left = total / 2;
    let right = total - left;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
}

fn pad_to_width(s: &str, w: usize) -> String {
    let pad = w.saturating_sub(line_width(s));
    format!("{}{}", s, " ".repeat(pad))
}

fn center_x(i: usize, name_w: usize, gap: usize) -> usize {
    i * (name_w + gap) + name_w / 2
}

struct Frag {
    x: usize,
    text: String,
}

fn assemble(mut frags: Vec<Frag>) -> String {
    frags.sort_by_key(|f| f.x);
    let mut line = String::new();
    let mut cur = 0usize;
    for f in frags {
        if f.x > cur {
            line.push_str(&" ".repeat(f.x - cur));
            cur = f.x;
        } else if f.x < cur {
            continue;
        }
        line.push_str(&f.text);
        cur += line_width(&f.text);
    }
    line
}

fn build_header(actor_disp: &[String], name_w: usize, gap: usize) -> String {
    let mut s = String::new();
    for (i, name) in actor_disp.iter().enumerate() {
        s.push_str(&pad_center(name, name_w));
        if i + 1 < actor_disp.len() {
            s.push_str(&" ".repeat(gap));
        }
    }
    s
}

fn row_with_content(n: usize, name_w: usize, gap: usize, content_x: usize, content_text: &str) -> String {
    let content_w = line_width(content_text);
    let content_end = content_x + content_w;
    let mut frags: Vec<Frag> = Vec::new();
    for i in 0..n {
        let cx = center_x(i, name_w, gap);
        if cx >= content_x && cx < content_end {
            continue;
        }
        frags.push(Frag { x: cx, text: "│".to_string() });
    }
    frags.push(Frag { x: content_x, text: content_text.to_string() });
    assemble(frags)
}

fn message_content(from_disp: &str, to_disp: &str, label_disp: &str, forward: bool) -> String {
    let mid = if label_disp.is_empty() {
        "────".to_string()
    } else {
        format!("──{}──", label_disp)
    };
    if forward {
        format!("{}{}►{}", from_disp, mid, to_disp)
    } else {
        format!("{}◄{}{}", to_disp, mid, from_disp)
    }
}

fn self_loop_rows(n: usize, name_w: usize, gap: usize, idx: usize, label_disp: &str) -> Vec<String> {
    let cx = center_x(idx, name_w, gap);
    let row2_text = if label_disp.is_empty() { "│".to_string() } else { format!("│ {}", label_disp) };
    vec![
        row_with_content(n, name_w, gap, cx, "─┐"),
        row_with_content(n, name_w, gap, cx, &row2_text),
        row_with_content(n, name_w, gap, cx, "◄┘"),
    ]
}

fn try_render(actors: &[String], messages: &[Message], budget: Option<usize>, width: usize) -> Option<Vec<String>> {
    let actors_disp: Vec<String> = actors.iter().map(|a| trunc(a, budget)).collect();
    let name_w = actors_disp.iter().map(|s| line_width(s)).max().unwrap_or(1).max(1);
    let n = actors.len();

    let mut rows = vec![build_header(&actors_disp, name_w, GAP)];

    for m in messages {
        let from_idx = actors.iter().position(|a| a == &m.from)?;
        let to_idx = actors.iter().position(|a| a == &m.to)?;
        let label_disp = trunc(&m.label, budget);

        if from_idx == to_idx {
            rows.extend(self_loop_rows(n, name_w, GAP, from_idx, &label_disp));
            continue;
        }

        let forward = to_idx > from_idx;
        let left_idx = from_idx.min(to_idx);
        let content_x = center_x(left_idx, name_w, GAP);
        let content = message_content(&actors_disp[from_idx], &actors_disp[to_idx], &label_disp, forward);
        rows.push(row_with_content(n, name_w, GAP, content_x, &content));
    }

    let overall_w = rows.iter().map(|r| line_width(r)).max().unwrap_or(0);
    if overall_w > width {
        return None;
    }

    Some(rows.iter().map(|r| pad_to_width(r, overall_w)).collect())
}

pub fn layout(diagram: &Diagram, width: u16) -> Result<Vec<String>, Fallback> {
    let (actors, messages) = match diagram {
        Diagram::Sequence { actors, messages } => (actors, messages),
        _ => return Err(Fallback::Empty { kind: "sequence".to_string() }),
    };

    if actors.is_empty() {
        return Err(Fallback::Empty { kind: "sequence".to_string() });
    }

    let natural_name_w = actors.iter().map(|a| line_width(a)).max().unwrap_or(1).max(1);
    let natural_label_w = messages.iter().map(|m| line_width(&m.label)).max().unwrap_or(0);
    let natural_max = natural_name_w.max(natural_label_w).max(1);

    let mut budget: Option<usize> = None;
    let max_attempts = natural_max.saturating_add(1);
    for _ in 0..=max_attempts {
        if let Some(lines) = try_render(actors, messages, budget, width as usize) {
            return Ok(lines);
        }
        let next = match budget {
            None => natural_max.saturating_sub(1).max(MIN_BUDGET),
            Some(b) if b > MIN_BUDGET => b.saturating_sub(1),
            _ => break,
        };
        if Some(next) == budget {
            break;
        }
        budget = Some(next);
    }

    Err(Fallback::Overflow { kind: "sequence".to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::parse;

    fn joined(lines: &[String]) -> String {
        lines.join("\n")
    }

    #[test]
    fn test_design_md_fixture_full_sequence() {
        // Verbatim block from this repo's own
        // .kiro/specs/spec-viewer/design.md, System Flows ->
        // "문서 선택 -> 렌더 -> 변경 반영". Rendering it through the real
        // parse::parse() (task 2.5, with the id-to-alias resolution bugfix
        // from task 2.7's retry) is the mandatory DONE criteria for 2.7.
        //
        // parse::seq now resolves a raw short id used in a message (e.g.
        // `U` in `U->>A: ...`) back to the display alias declared via
        // `participant U as User`, so the 5 declared participants collapse
        // onto exactly 5 columns instead of 10.
        let src = "sequenceDiagram\n    participant U as User\n    participant A as App\n    participant L as Loader\n    participant M as Markdown\n    participant W as Watcher\n    U->>A: SelectNode(id)\n    A->>L: read_doc(path)\n    L-->>A: text | DocError\n    A->>M: render(text, width)\n    M-->>A: Rendered\n    A->>A: scroll=0, search cleared\n    W-->>A: FsEvent(paths)\n    A->>A: classify(doc | meta | tree)\n    A->>L: reload(affected)\n    A->>M: render\n    A->>A: scroll = min(prev, len-1)";

        let diagram = parse::parse(src).unwrap();
        let (actors, messages) = match &diagram {
            Diagram::Sequence { actors, messages } => (actors, messages),
            _ => panic!("expected a Sequence diagram"),
        };
        assert_eq!(
            actors,
            &["User", "App", "Loader", "Markdown", "Watcher"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(messages.len(), 11);

        let lines = layout(&diagram, 200).expect("fixture must fit at width 200");
        let text = joined(&lines);

        // Column headers appear in declaration order, and the raw short ids
        // no longer appear anywhere as separate columns.
        let header = &lines[0];
        let mut search_from = 0usize;
        for name in ["User", "App", "Loader", "Markdown", "Watcher"] {
            let pos = header[search_from..]
                .find(name)
                .unwrap_or_else(|| panic!("missing header {name} (searching from byte {search_from}) in: {header}"));
            search_from += pos + name.len();
        }

        // Lifelines run under every actor for a row where that actor takes
        // no part in the message (e.g. the very first message row, User->>App,
        // leaves Loader/Markdown/Watcher's lifelines untouched).
        let first_msg_row = &lines[1];
        assert!(first_msg_row.contains('│'), "expected lifelines to persist on a message row: {first_msg_row}");

        // 8 normal messages (1 row each) + 3 self messages (3 rows each) + 1 header.
        assert_eq!(lines.len(), 1 + 8 + 3 * 3);

        // Messages render in declaration order: SelectNode(id) then
        // read_doc(path) then render(text, width) (spot-check a few).
        let row_of = |needle: &str| lines.iter().position(|l| l.contains(needle)).unwrap_or_else(|| panic!("missing {needle} in:\n{text}"));
        let r_select = row_of("SelectNode(id)");
        let r_read = row_of("read_doc(path)");
        let r_render1 = row_of("render(text, width)");
        assert!(r_select < r_read, "SelectNode(id) must render before read_doc(path)");
        assert!(r_read < r_render1, "read_doc(path) must render before render(text, width)");

        // The first message must resolve onto the real User/App columns,
        // not disconnected "U"/"A" columns.
        let select_row = &lines[r_select];
        assert!(select_row.contains("User"), "SelectNode(id) must be attributed to User: {select_row}");
        assert!(select_row.contains("App"), "SelectNode(id) must be attributed to App: {select_row}");

        // The 3 self-messages each render as a loop distinct from a normal
        // connector, must land specifically on the App column (not some
        // other actor, and not a disconnected "A" column), and must not
        // appear to reach any other actor's column.
        let other_actors = ["User", "Loader", "Markdown", "Watcher"];
        let name_w = actors.iter().map(|a| line_width(a)).max().unwrap().max(1);
        let app_idx = actors.iter().position(|a| a == "App").expect("App must be an actor");
        let app_col = center_x(app_idx, name_w, GAP);
        for label in ["scroll=0, search cleared", "classify(doc | meta | tree)", "scroll = min(prev, len-1)"] {
            let r = row_of(label);
            assert!(r >= 1, "self message row must come after the header");
            let block = &lines[r - 1..=r + 1];
            let block_text = joined(block);
            assert!(
                block_text.contains("─┐") && block_text.contains("◄┘"),
                "self message {label} must render the loop glyphs, got:\n{block_text}"
            );
            let loop_byte_idx = lines[r - 1].find("─┐").expect("loop open glyph must be present");
            let loop_col = line_width(&lines[r - 1][..loop_byte_idx]);
            assert_eq!(loop_col, app_col, "self message {label} loop must sit on the App column, got:\n{block_text}");
            for other in other_actors {
                assert!(
                    !block_text.contains(other),
                    "self message {label} loop must not reach a different actor's column ({other}), got:\n{block_text}"
                );
            }
        }
    }

    #[test]
    fn test_basic_two_actor_message() {
        let src = "sequenceDiagram\nparticipant A\nparticipant B\nA->>B: hello";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80).unwrap();
        let header = &lines[0];
        let a_pos = header.find('A').unwrap();
        let b_pos = header.find('B').unwrap();
        assert!(a_pos < b_pos, "A must render as the first (left) column, B second");

        let msg_row = &lines[1];
        assert!(msg_row.contains('A'), "message row must name A: {msg_row}");
        assert!(msg_row.contains('B'), "message row must name B: {msg_row}");
        assert!(msg_row.contains("hello"), "message row must show the label: {msg_row}");
        assert!(msg_row.contains('►'), "forward message must use the ► arrowhead: {msg_row}");
        let a_msg_pos = msg_row.find('A').unwrap();
        let b_msg_pos = msg_row.find('B').unwrap();
        assert!(a_msg_pos < b_msg_pos, "A->>B must render A before B: {msg_row}");
    }

    #[test]
    fn test_self_message_isolated_and_not_misattributed() {
        let src = "sequenceDiagram\nparticipant A\nparticipant B\nA->>A: loop test";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80).unwrap();
        assert_eq!(lines.len(), 1 + 3, "expected header + 3 loop rows");

        let loop_block = joined(&lines[1..]);
        assert!(loop_block.contains("loop test"), "expected the self message label: {loop_block}");
        assert!(loop_block.contains("─┐") && loop_block.contains("◄┘"), "expected loop glyphs: {loop_block}");
        assert!(!loop_block.contains('►'), "a self message loop must not use the forward-connector arrowhead: {loop_block}");
        assert!(!loop_block.contains('B'), "A's self loop must not reach B's column: {loop_block}");
    }

    #[test]
    fn test_width_overflow_falls_back() {
        let src = "sequenceDiagram\nparticipant A\nparticipant B\nparticipant C\nparticipant D\nparticipant E\nparticipant F\nparticipant G\nparticipant H\nA->>B: hi";
        let diagram = parse::parse(src).unwrap();
        let res = layout(&diagram, 20);
        assert!(matches!(res, Err(Fallback::Overflow { .. })), "expected overflow fallback, got {:?}", res);
    }

    #[test]
    fn test_korean_actor_and_label_no_panic_correct_width() {
        let src = "sequenceDiagram\nparticipant 사용자\nparticipant 시스템\n사용자->>시스템: 안녕하세요 메시지 내용";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80).unwrap();
        let text = joined(&lines);
        assert!(text.contains("사용자"));
        assert!(text.contains("시스템"));
        assert!(text.contains("안녕하세요 메시지 내용"));
        for l in &lines {
            assert!(line_width(l) <= 80, "line exceeds width 80: {l}");
        }
    }

    #[test]
    fn test_declaration_order_preserved_across_varying_pairs() {
        let src = "sequenceDiagram\nparticipant A\nparticipant B\nparticipant C\nA->>C: first\nC->>B: second\nB->>A: third\nA->>B: fourth";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 100).unwrap();
        let row_of = |needle: &str| lines.iter().position(|l| l.contains(needle)).unwrap();
        let r1 = row_of("first");
        let r2 = row_of("second");
        let r3 = row_of("third");
        let r4 = row_of("fourth");
        assert!(r1 < r2 && r2 < r3 && r3 < r4, "message rows must preserve declaration order: {:?}", lines);
    }

    #[test]
    fn test_zero_content_defensive_empty_fallback() {
        let diagram = Diagram::Sequence { actors: Vec::new(), messages: Vec::new() };
        let res = layout(&diagram, 80);
        assert!(matches!(res, Err(Fallback::Empty { .. })));
    }

    #[test]
    fn test_backward_message_uses_reverse_arrowhead() {
        let src = "sequenceDiagram\nparticipant A\nparticipant B\nB-->>A: reply";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80).unwrap();
        let msg_row = &lines[1];
        assert!(msg_row.contains('◄'), "backward message (B-->>A) must use the ◄ arrowhead: {msg_row}");
        assert!(msg_row.contains("reply"));
    }
}
