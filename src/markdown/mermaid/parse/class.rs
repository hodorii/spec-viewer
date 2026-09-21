use super::*;

fn classify_member(member: &str) -> bool {
    member.contains('(')
}

fn split_by_markers(body: &str) -> Vec<String> {
    let chars: Vec<char> = body.chars().collect();
    let mut result = Vec::new();
    let mut current = String::new();
    for (i, c) in chars.iter().enumerate() {
        let is_marker = matches!(c, '+' | '-' | '#' | '~');
        let prev_is_boundary = i == 0 || chars[i - 1].is_whitespace();
        if is_marker && prev_is_boundary && !current.trim().is_empty() {
            result.push(current.trim().to_string());
            current.clear();
        }
        current.push(*c);
    }
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }
    result
}

fn split_members(body: &str) -> (Vec<String>, Vec<String>) {
    let raw_members: Vec<String> = if body.trim_start().starts_with(['+', '-', '#', '~']) {
        split_by_markers(body)
    } else {
        body.split(['\n', ';'])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };

    let mut attrs = Vec::new();
    let mut methods = Vec::new();
    for member in raw_members {
        if classify_member(&member) {
            methods.push(member);
        } else {
            attrs.push(member);
        }
    }
    (attrs, methods)
}

fn relationship_style(op: &str) -> EdgeStyle {
    match op {
        "<|--" | "--|>" => EdgeStyle::Inherit,
        "*--" | "--*" => EdgeStyle::Compose,
        "o--" | "--o" => EdgeStyle::Aggregate,
        "-->" => EdgeStyle::Arrow,
        _ => EdgeStyle::Line,
    }
}

/// mermaid classDiagram 관계 다중성 표기(`"1"`, `"N"`, `"0..1"`, `"0..*"` 등,
/// 큰따옴표로 감싼 구간)를 걷어낸다. 이 크레이트는 다중성을 렌더링하지 않으므로
/// (ER 의 기호 카디널리티와 달리 classDiagram 은 별도 모델이 없다) 그냥
/// 버린다 — 남겨두면 좌/우 식별자에 섞여 `Organization "1"` 같은 가짜 노드가
/// 생긴다(16.1 재현 실측: gitea-github-schema.md, 15개여야 할 클래스가 42개로).
fn strip_quoted_multiplicity(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_quotes = false;
    for ch in s.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
            continue;
        }
        if !in_quotes {
            out.push(ch);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_relationship_line(line: &str) -> Option<(String, String, EdgeStyle, Option<String>)> {
    let (rel_part, label) = match line.split_once(':') {
        Some((rel, lbl)) => (rel.trim(), Some(lbl.trim().to_string())),
        None => (line.trim(), None),
    };

    let ops = ["<|--", "--|>", "*--", "--*", "o--", "--o", "-->", "--"];
    for op in ops {
        if let Some(idx) = rel_part.find(op) {
            let left = strip_quoted_multiplicity(rel_part[..idx].trim());
            let right = strip_quoted_multiplicity(rel_part[idx + op.len()..].trim());
            if left.is_empty() || right.is_empty() {
                continue;
            }
            return Some((left, right, relationship_style(op), label));
        }
    }
    None
}

pub fn parse(src: &str) -> Result<Diagram, Fallback> {
    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();

    let mut in_class: Option<(String, String)> = None;
    // 16.2: `%%` 로 시작하는 줄(독립 주석)뿐 아니라 멤버 뒤에 붙는 인라인
    // 주석(`+Int type  %% 0: User, 1: Organization`)도 잘라낸다 — 그대로
    // 두면 멤버 텍스트에 섞여 상자 폭을 불필요하게 늘린다.
    let mut lines = src
        .lines()
        .map(|l| l.split("%%").next().unwrap_or("").trim())
        .filter(|l| !l.is_empty());
    lines.next();

    fn push_class(nodes: &mut Vec<Node>, name: &str, body: &str) {
        let (attrs, methods) = split_members(body);
        nodes.push(Node { id: name.to_string(), shape: Shape::Class { name: name.to_string(), attrs, methods } });
    }

    for line in lines {
        if let Some((name, buf)) = in_class.as_mut() {
            if line == "}" {
                push_class(&mut nodes, name, buf);
                in_class = None;
                continue;
            }
            buf.push_str(line);
            buf.push('\n');
            continue;
        }

        if let Some(rest) = line.strip_prefix("class ") {
            let rest = rest.trim();
            if let Some(brace_idx) = rest.find('{') {
                let name = rest[..brace_idx].trim().to_string();
                let after = rest[brace_idx + 1..].trim();
                if let Some(end) = after.find('}') {
                    push_class(&mut nodes, &name, &after[..end]);
                } else {
                    in_class = Some((name, after.to_string()));
                }
            } else if !rest.is_empty() {
                push_class(&mut nodes, rest, "");
            }
            continue;
        }

        if let Some((left, right, style, label)) = parse_relationship_line(line) {
            for id in [&left, &right] {
                if !nodes.iter().any(|n| &n.id == id) {
                    nodes.push(Node {
                        id: id.clone(),
                        shape: Shape::Class { name: id.clone(), attrs: Vec::new(), methods: Vec::new() },
                    });
                }
            }
            edges.push(Edge { from: left, to: right, label, style });
        }
    }

    if nodes.is_empty() {
        return Err(Fallback::Empty { kind: "class".to_string() });
    }

    Ok(Diagram::Graph { dir: Dir::TB, nodes, edges, groups: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_class_compartments_single_line() {
        let src = "classDiagram\n  class Foo { +int x -bar() }";
        let res = parse(src).unwrap();
        if let Diagram::Graph { nodes, .. } = res {
            let foo = nodes.iter().find(|n| n.id == "Foo").unwrap();
            match &foo.shape {
                Shape::Class { attrs, methods, .. } => {
                    assert_eq!(attrs, &vec!["+int x".to_string()]);
                    assert_eq!(methods, &vec!["-bar()".to_string()]);
                }
                _ => panic!("not a class"),
            }
        } else {
            panic!("not a graph");
        }
    }

    #[test]
    fn test_trailing_inline_comment_stripped_from_member() {
        // 16.2: 줄 앞이 아니라 멤버 뒤에 붙는 `%%` 인라인 주석(실측:
        // gitea-github-schema.md `+Int type  %% 0: User, 1: Organization`) —
        // 줄 전체가 `%%` 로 시작하는 경우만 걸러내던 기존 필터로는 못 잡고
        // 그대로 멤버 텍스트에 섞여 폭을 불필요하게 늘렸다.
        let src = "classDiagram\n  class Foo {\n    +Int type  %% 0: User, 1: Organization\n  }";
        let res = parse(src).unwrap();
        if let Diagram::Graph { nodes, .. } = res {
            let foo = nodes.iter().find(|n| n.id == "Foo").unwrap();
            match &foo.shape {
                Shape::Class { attrs, .. } => {
                    assert_eq!(attrs, &vec!["+Int type".to_string()], "trailing %% comment must be stripped");
                }
                _ => panic!("not a class"),
            }
        } else {
            panic!("not a graph");
        }
    }

    #[test]
    fn test_class_compartments_multiline() {
        let src = "classDiagram\n  class Foo {\n    +int x\n    +bar()\n  }";
        let res = parse(src).unwrap();
        if let Diagram::Graph { nodes, .. } = res {
            let foo = nodes.iter().find(|n| n.id == "Foo").unwrap();
            match &foo.shape {
                Shape::Class { attrs, methods, .. } => {
                    assert_eq!(attrs, &vec!["+int x".to_string()]);
                    assert_eq!(methods, &vec!["+bar()".to_string()]);
                }
                _ => panic!("not a class"),
            }
        } else {
            panic!("not a graph");
        }
    }

    #[test]
    fn test_relationship_kinds() {
        let src = "classDiagram\n  Animal <|-- Dog\n  Car *-- Engine\n  Car o-- Wheel\n  A --> B : uses";
        let res = parse(src).unwrap();
        if let Diagram::Graph { edges, .. } = res {
            assert_eq!(edges[0].style, EdgeStyle::Inherit);
            assert_eq!(edges[1].style, EdgeStyle::Compose);
            assert_eq!(edges[2].style, EdgeStyle::Aggregate);
            assert_eq!(edges[3].style, EdgeStyle::Arrow);
            assert_eq!(edges[3].label, Some("uses".to_string()));
        } else {
            panic!("not a graph");
        }
    }

    #[test]
    fn test_quoted_multiplicity_does_not_leak_into_node_id() {
        // 16.1 재현 과정에서 실측: gitea-github-schema.md 의
        // `Organization "1" *-- "N" Team : owns` 같은 관계 다중성 표기가
        // 그대로 좌/우 식별자에 섞여 `Organization "1"`/`"N" Team` 이라는
        // 가짜 클래스 노드를 만들었다 — 15개여야 할 클래스가 42개로 부풀어
        // (실측) 어떤 폭에서도 못 들어가는 원인이었다.
        let src = "classDiagram\n  Organization \"1\" *-- \"N\" Team : owns";
        let res = parse(src).unwrap();
        if let Diagram::Graph { nodes, edges, .. } = res {
            let ids: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
            assert_eq!(ids, vec!["Organization", "Team"], "quoted multiplicities must not leak into node ids: {ids:?}");
            assert_eq!(edges[0].from, "Organization");
            assert_eq!(edges[0].to, "Team");
            assert_eq!(edges[0].label, Some("owns".to_string()));
        } else {
            panic!("not a graph");
        }
    }

    #[test]
    fn test_zero_nodes_is_empty_fallback() {
        let src = "classDiagram\n  %% nothing";
        let res = parse(src);
        assert!(matches!(res, Err(Fallback::Empty { .. })));
    }
}
