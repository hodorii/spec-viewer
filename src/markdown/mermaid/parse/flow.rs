use super::*;

fn parse_dir(header: &str) -> Dir {
    match header.split_whitespace().nth(1) {
        Some("LR") | Some("RL") => Dir::LR,
        _ => Dir::TB,
    }
}

fn parse_node_token(token: &str) -> (String, Option<String>) {
    let token = token.trim();
    if let Some(start) = token.find("((") {
        if let Some(end) = token.rfind("))") {
            if end > start {
                let id = token[..start].trim().to_string();
                let label = token[start + 2..end].trim().to_string();
                return (id, Some(label));
            }
        }
    }
    for (open, close) in [('[', ']'), ('(', ')'), ('{', '}')] {
        if let Some(start) = token.find(open) {
            if let Some(end) = token.rfind(close) {
                if end > start {
                    let id = token[..start].trim().to_string();
                    let label = token[start + 1..end].trim().to_string();
                    return (id, Some(label));
                }
            }
        }
    }
    (token.to_string(), None)
}

fn upsert_node(nodes: &mut Vec<Node>, id: &str, label: Option<String>) {
    if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
        if let (Some(l), Shape::Box(existing)) = (&label, &node.shape) {
            if existing == id {
                node.shape = Shape::Box(l.clone());
            }
        }
        return;
    }
    let shape = Shape::Box(label.unwrap_or_else(|| id.to_string()));
    nodes.push(Node { id: id.to_string(), shape });
}

struct RawEdge {
    from: String,
    from_label: Option<String>,
    to: String,
    to_label: Option<String>,
    label: Option<String>,
    style: EdgeStyle,
}

fn parse_edges(line: &str) -> Vec<RawEdge> {
    if let Some(idx) = line.find("-->") {
        let left = &line[..idx];
        let right = &line[idx + 3..];

        if let Some(rest) = right.strip_prefix('|') {
            if let Some(end_pipe) = rest.find('|') {
                let label = rest[..end_pipe].trim().to_string();
                let (from, from_label) = parse_node_token(left);
                let (to, to_label) = parse_node_token(&rest[end_pipe + 1..]);
                return vec![RawEdge {
                    from,
                    from_label,
                    to,
                    to_label,
                    label: Some(label),
                    style: EdgeStyle::Arrow,
                }];
            }
        }

        if let Some(dash_idx) = left.find("--") {
            let (from, from_label) = parse_node_token(&left[..dash_idx]);
            let label = left[dash_idx + 2..].trim().to_string();
            let (to, to_label) = parse_node_token(right);
            return vec![RawEdge {
                from,
                from_label,
                to,
                to_label,
                label: if label.is_empty() { None } else { Some(label) },
                style: EdgeStyle::Arrow,
            }];
        }

        let parts: Vec<&str> = line.split("-->").collect();
        return parts
            .windows(2)
            .map(|w| {
                let (from, from_label) = parse_node_token(w[0]);
                let (to, to_label) = parse_node_token(w[1]);
                RawEdge { from, from_label, to, to_label, label: None, style: EdgeStyle::Arrow }
            })
            .collect();
    }

    if line.contains("---") {
        let parts: Vec<&str> = line.split("---").collect();
        return parts
            .windows(2)
            .map(|w| {
                let (from, from_label) = parse_node_token(w[0]);
                let (to, to_label) = parse_node_token(w[1]);
                RawEdge { from, from_label, to, to_label, label: None, style: EdgeStyle::Line }
            })
            .collect();
    }

    Vec::new()
}

pub fn parse(src: &str) -> Result<Diagram, Fallback> {
    let mut lines = src
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));

    let header = lines
        .next()
        .ok_or_else(|| Fallback::Empty { kind: "flowchart".to_string() })?;
    let dir = parse_dir(header);

    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut groups: Vec<Subgraph> = Vec::new();
    let mut current_subgraph: Option<(String, Vec<String>)> = None;

    for line in lines {
        if let Some(title) = line.strip_prefix("subgraph") {
            current_subgraph = Some((title.trim().to_string(), Vec::new()));
            continue;
        }
        if line == "end" {
            if let Some((title, members)) = current_subgraph.take() {
                groups.push(Subgraph { title, members });
            }
            continue;
        }

        let raw_edges = parse_edges(line);
        if !raw_edges.is_empty() {
            for raw in raw_edges {
                upsert_node(&mut nodes, &raw.from, raw.from_label);
                upsert_node(&mut nodes, &raw.to, raw.to_label);
                if let Some((_, members)) = current_subgraph.as_mut() {
                    members.push(raw.from.clone());
                    members.push(raw.to.clone());
                }
                edges.push(Edge {
                    from: raw.from,
                    to: raw.to,
                    label: raw.label,
                    style: raw.style,
                });
            }
            continue;
        }

        let (id, label) = parse_node_token(line);
        if !id.is_empty() {
            upsert_node(&mut nodes, &id, label);
            if let Some((_, members)) = current_subgraph.as_mut() {
                members.push(id);
            }
        }
    }

    if nodes.is_empty() {
        return Err(Fallback::Empty { kind: "flowchart".to_string() });
    }

    Ok(Diagram::Graph { dir, nodes, edges, groups })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boundary_map() {
        let src = "graph LR\n  ValueChain --> BizProcess\n  BizProcess --> Requirements\n  Requirements --> Design\n  Design --> Tasks\n  Tasks --> Impl\n  Impl --> Validate\n  Validate --> Verify\n  Verify --> Done\n  BizProcess --> Design\n  Requirements --> Design\n  Design --> Tasks\n  Tasks --> Impl\n  Impl --> Validate\n  Validate --> Verify";
        let res = parse(src).unwrap();
        if let Diagram::Graph { dir, nodes, edges, .. } = res {
            assert_eq!(dir, Dir::LR);
            assert!(nodes.len() >= 9);
            assert!(edges.len() >= 14);
        } else {
            panic!("Not a graph");
        }
    }

    #[test]
    fn test_edge_label() {
        let src = "graph LR\n  A -->|go| B";
        let res = parse(src).unwrap();
        if let Diagram::Graph { edges, .. } = res {
            assert_eq!(edges[0].label, Some("go".to_string()));
        } else {
            panic!("Not a graph");
        }
    }

    #[test]
    fn test_edge_label_dash_form() {
        let src = "graph LR\n  A -- go --> B";
        let res = parse(src).unwrap();
        if let Diagram::Graph { edges, .. } = res {
            assert_eq!(edges[0].label, Some("go".to_string()));
            assert_eq!(edges[0].from, "A");
            assert_eq!(edges[0].to, "B");
        } else {
            panic!("Not a graph");
        }
    }

    #[test]
    fn test_subgraph_members() {
        let src = "graph LR\n  subgraph G1\n    A\n    B\n  end";
        let res = parse(src).unwrap();
        if let Diagram::Graph { groups, .. } = res {
            assert_eq!(groups[0].title, "G1");
            assert!(groups[0].members.contains(&"A".to_string()));
            assert!(groups[0].members.contains(&"B".to_string()));
        } else {
            panic!("Not a graph");
        }
    }

    #[test]
    fn test_node_label_bracket_syntax() {
        let src = "graph TB\n  A[Start Node] --> B(Round)\n  C{Diamond}\n  D((Circle))";
        let res = parse(src).unwrap();
        if let Diagram::Graph { dir, nodes, .. } = res {
            assert_eq!(dir, Dir::TB);
            let a = nodes.iter().find(|n| n.id == "A").unwrap();
            assert_eq!(a.shape, Shape::Box("Start Node".to_string()));
            let b = nodes.iter().find(|n| n.id == "B").unwrap();
            assert_eq!(b.shape, Shape::Box("Round".to_string()));
            let c = nodes.iter().find(|n| n.id == "C").unwrap();
            assert_eq!(c.shape, Shape::Box("Diamond".to_string()));
            let d = nodes.iter().find(|n| n.id == "D").unwrap();
            assert_eq!(d.shape, Shape::Box("Circle".to_string()));
        } else {
            panic!("Not a graph");
        }
    }

    #[test]
    fn test_bare_node_no_shape_defaults_to_id() {
        let src = "graph TB\n  A --> B";
        let res = parse(src).unwrap();
        if let Diagram::Graph { nodes, .. } = res {
            let a = nodes.iter().find(|n| n.id == "A").unwrap();
            assert_eq!(a.shape, Shape::Box("A".to_string()));
        } else {
            panic!("Not a graph");
        }
    }

    #[test]
    fn test_line_style_edge() {
        let src = "graph TB\n  A --- B";
        let res = parse(src).unwrap();
        if let Diagram::Graph { edges, .. } = res {
            assert_eq!(edges[0].style, EdgeStyle::Line);
        } else {
            panic!("Not a graph");
        }
    }

    #[test]
    fn test_zero_nodes_is_empty_fallback() {
        let src = "graph TB\n  %% just a comment";
        let res = parse(src);
        assert!(matches!(res, Err(Fallback::Empty { .. })));
    }
}
