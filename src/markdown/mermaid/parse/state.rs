use super::*;

fn upsert_state(nodes: &mut Vec<Node>, id: &str, shape: Shape) {
    if nodes.iter().any(|n| n.id == id && n.shape == shape) {
        return;
    }
    if id != "[*]" && nodes.iter().any(|n| n.id == id) {
        return;
    }
    nodes.push(Node { id: id.to_string(), shape });
}

pub fn parse(src: &str) -> Result<Diagram, Fallback> {
    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();

    let mut lines = src
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    lines.next();

    for line in lines {
        if let Some(idx) = line.find("-->") {
            let from = line[..idx].trim().to_string();
            let rest = line[idx + 3..].trim();
            let (to, label) = match rest.split_once(':') {
                Some((t, l)) => (t.trim().to_string(), Some(l.trim().to_string())),
                None => (rest.to_string(), None),
            };

            let from_shape = if from == "[*]" { Shape::Start } else { Shape::State(from.clone()) };
            let to_shape = if to == "[*]" { Shape::End } else { Shape::State(to.clone()) };
            upsert_state(&mut nodes, &from, from_shape);
            upsert_state(&mut nodes, &to, to_shape);

            edges.push(Edge { from, to, label, style: EdgeStyle::Transition });
            continue;
        }

        if !line.is_empty() && line != "[*]" {
            upsert_state(&mut nodes, line, Shape::State(line.to_string()));
        }
    }

    if nodes.is_empty() {
        return Err(Fallback::Empty { kind: "state".to_string() });
    }

    Ok(Diagram::Graph { dir: Dir::TB, nodes, edges, groups: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_and_end_pseudo_states() {
        let src = "stateDiagram\n  [*] --> Idle\n  Idle --> [*]";
        let res = parse(src).unwrap();
        if let Diagram::Graph { nodes, edges, .. } = res {
            assert!(nodes.iter().any(|n| n.shape == Shape::Start));
            assert!(nodes.iter().any(|n| n.shape == Shape::End));
            assert!(nodes.iter().any(|n| n.shape == Shape::State("Idle".to_string())));
            assert_eq!(edges.len(), 2);
            assert_eq!(edges[0].style, EdgeStyle::Transition);
        } else {
            panic!("not a graph");
        }
    }

    #[test]
    fn test_transition_with_label() {
        let src = "stateDiagram-v2\n  Idle --> Running : start";
        let res = parse(src).unwrap();
        if let Diagram::Graph { edges, .. } = res {
            assert_eq!(edges[0].from, "Idle");
            assert_eq!(edges[0].to, "Running");
            assert_eq!(edges[0].label, Some("start".to_string()));
        } else {
            panic!("not a graph");
        }
    }

    #[test]
    fn test_zero_nodes_is_empty_fallback() {
        let src = "stateDiagram\n  %% nothing";
        let res = parse(src);
        assert!(matches!(res, Err(Fallback::Empty { .. })));
    }
}
