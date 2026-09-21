use super::*;

fn looks_like_title(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

fn ensure_box(nodes: &mut Vec<Node>, title: &str, body: Option<&str>) {
    if nodes.iter().any(|n| n.id == title) {
        return;
    }
    let attrs = match body {
        Some(b) if !b.is_empty() => vec![b.to_string()],
        _ => Vec::new(),
    };
    nodes.push(Node {
        id: title.to_string(),
        shape: Shape::Class { name: title.to_string(), attrs, methods: Vec::new() },
    });
}

pub fn parse(src: &str) -> Result<Diagram, Fallback> {
    let mut lines = src
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    lines.next();

    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();

    for line in lines {
        let mut handled_as_edge = false;
        for (sep, style) in [("-->", EdgeStyle::Arrow), ("->", EdgeStyle::Arrow), (":", EdgeStyle::Line)] {
            if let Some(idx) = line.find(sep) {
                let left = line[..idx].trim();
                let right = line[idx + sep.len()..].trim();
                if looks_like_title(left) && looks_like_title(right) {
                    ensure_box(&mut nodes, left, None);
                    ensure_box(&mut nodes, right, None);
                    edges.push(Edge { from: left.to_string(), to: right.to_string(), label: None, style });
                    handled_as_edge = true;
                    break;
                }
            }
        }
        if handled_as_edge {
            continue;
        }

        let mut parts = line.splitn(2, char::is_whitespace);
        let title = parts.next().unwrap_or("").trim();
        let rest = parts.next().unwrap_or("").trim();
        if title.is_empty() {
            continue;
        }
        ensure_box(&mut nodes, title, if rest.is_empty() { None } else { Some(rest) });
    }

    if nodes.is_empty() {
        return Err(Fallback::Empty { kind: "generic".to_string() });
    }

    Ok(Diagram::Graph { dir: Dir::TB, nodes, edges, groups: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gantt_fixture_line_per_box() {
        let src = "gantt\n    title Adoption Timeline\n    section Planning\n    Kickoff :done, 2024-01-01, 3d\n    Research :active, 2024-01-04, 5d\n    Kickoff --> Research";
        let res = parse(src).unwrap();
        if let Diagram::Graph { dir, nodes, edges, .. } = res {
            assert_eq!(dir, Dir::TB);
            let kickoff = nodes.iter().find(|n| n.id == "Kickoff").unwrap();
            match &kickoff.shape {
                Shape::Class { attrs, methods, .. } => {
                    assert_eq!(attrs, &vec![":done, 2024-01-01, 3d".to_string()]);
                    assert!(methods.is_empty());
                }
                _ => panic!("expected class shape"),
            }
            assert!(edges.iter().any(|e| e.from == "Kickoff" && e.to == "Research" && e.style == EdgeStyle::Arrow));
        } else {
            panic!("not a graph");
        }
    }

    #[test]
    fn test_bare_colon_relation_between_titles() {
        let src = "pie\n  A\n  B\n  A : B";
        let res = parse(src).unwrap();
        if let Diagram::Graph { edges, .. } = res {
            assert!(edges.iter().any(|e| e.from == "A" && e.to == "B" && e.style == EdgeStyle::Line));
        } else {
            panic!("not a graph");
        }
    }

    #[test]
    fn test_zero_boxes_is_empty_fallback() {
        let src = "gantt\n  %% just a comment";
        let res = parse(src);
        assert!(matches!(res, Err(Fallback::Empty { kind }) if kind == "generic"));
    }
}
