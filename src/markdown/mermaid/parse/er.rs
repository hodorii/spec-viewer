use super::*;

fn parse_cardinality_line(line: &str) -> Option<(String, String, String, String, Option<String>)> {
    let (rel_part, label) = match line.split_once(':') {
        Some((rel, lbl)) => (rel.trim(), Some(lbl.trim().to_string())),
        None => (line, None),
    };

    let symbols = ["||--o{", "||--|{", "}o--||", "}|--||", "||--||", "}o--o{", "}|--|{", "o|--o|"];
    for sym in symbols {
        if let Some(idx) = rel_part.find(sym) {
            let left_entity = rel_part[..idx].trim().to_string();
            let right_entity = rel_part[idx + sym.len()..].trim().to_string();
            if let Some(mid) = sym.find("--") {
                let left_card = sym[..mid].to_string();
                let right_card = sym[mid + 2..].to_string();
                return Some((left_entity, right_entity, left_card, right_card, label));
            }
        }
    }
    None
}

fn parse_attribute_line(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    Some(line.to_string())
}

pub fn parse(src: &str) -> Result<Diagram, Fallback> {
    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();

    let mut in_entity: Option<(String, Vec<String>)> = None;
    let mut lines = src
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    lines.next();

    for line in lines {
        if let Some((name, fields)) = in_entity.as_mut() {
            if line == "}" {
                nodes.push(Node {
                    id: name.clone(),
                    shape: Shape::Entity { name: name.clone(), fields: std::mem::take(fields) },
                });
                in_entity = None;
                continue;
            }
            if let Some(attr) = parse_attribute_line(line) {
                fields.push(attr);
            }
            continue;
        }

        if let Some((left, right, left_card, right_card, label)) = parse_cardinality_line(line) {
            for entity in [&left, &right] {
                if !nodes.iter().any(|n| &n.id == entity) {
                    nodes.push(Node {
                        id: entity.clone(),
                        shape: Shape::Entity { name: entity.clone(), fields: Vec::new() },
                    });
                }
            }
            edges.push(Edge {
                from: left,
                to: right,
                label,
                style: EdgeStyle::Cardinality(left_card, right_card),
            });
            continue;
        }

        if let Some(brace_idx) = line.find('{') {
            let name = line[..brace_idx].trim().to_string();
            let rest = line[brace_idx + 1..].trim();
            if let Some(end) = rest.find('}') {
                let body = &rest[..end];
                let fields: Vec<String> = body
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .chunks(2)
                    .map(|c| c.join(" "))
                    .collect();
                nodes.push(Node { id: name.clone(), shape: Shape::Entity { name, fields } });
            } else {
                in_entity = Some((name, Vec::new()));
            }
        }
    }

    if nodes.is_empty() {
        return Err(Fallback::Empty { kind: "er".to_string() });
    }

    Ok(Diagram::Graph { dir: Dir::TB, nodes, edges, groups: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_attributes_inline() {
        let src = "erDiagram\n  CUSTOMER { string name string id }";
        let res = parse(src).unwrap();
        if let Diagram::Graph { nodes, .. } = res {
            let c = nodes.iter().find(|n| n.id == "CUSTOMER").unwrap();
            match &c.shape {
                Shape::Entity { name, fields } => {
                    assert_eq!(name, "CUSTOMER");
                    assert_eq!(fields, &vec!["string name".to_string(), "string id".to_string()]);
                }
                _ => panic!("not an entity"),
            }
        } else {
            panic!("not a graph");
        }
    }

    #[test]
    fn test_relationship_cardinality_and_label() {
        let src = "erDiagram\n  CUSTOMER ||--o{ ORDER : places";
        let res = parse(src).unwrap();
        if let Diagram::Graph { edges, nodes, .. } = res {
            assert_eq!(nodes.len(), 2);
            assert_eq!(edges[0].from, "CUSTOMER");
            assert_eq!(edges[0].to, "ORDER");
            assert_eq!(edges[0].label, Some("places".to_string()));
            assert_eq!(edges[0].style, EdgeStyle::Cardinality("||".to_string(), "o{".to_string()));
        } else {
            panic!("not a graph");
        }
    }

    #[test]
    fn test_multiline_entity_block() {
        let src = "erDiagram\n  CUSTOMER {\n    string name\n    string id\n  }";
        let res = parse(src).unwrap();
        if let Diagram::Graph { nodes, .. } = res {
            let c = nodes.iter().find(|n| n.id == "CUSTOMER").unwrap();
            match &c.shape {
                Shape::Entity { fields, .. } => {
                    assert_eq!(fields, &vec!["string name".to_string(), "string id".to_string()]);
                }
                _ => panic!("not an entity"),
            }
        } else {
            panic!("not a graph");
        }
    }

    #[test]
    fn test_zero_nodes_is_empty_fallback() {
        let src = "erDiagram\n  %% nothing here";
        let res = parse(src);
        assert!(matches!(res, Err(Fallback::Empty { .. })));
    }
}
