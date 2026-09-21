use super::*;

fn ensure_actor(actors: &mut Vec<String>, name: &str) {
    if !actors.iter().any(|a| a == name) {
        actors.push(name.to_string());
    }
}

fn parse_participant_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("participant ").or_else(|| line.strip_prefix("actor "))?;
    let rest = rest.trim();
    if let Some((id, alias)) = rest.split_once(" as ") {
        Some((id.trim().to_string(), alias.trim().to_string()))
    } else {
        Some((rest.to_string(), rest.to_string()))
    }
}

fn parse_message_line(line: &str) -> Option<(String, String, String)> {
    let arrows = ["-->>", "->>", "-->", "->"];
    for arrow in arrows {
        if let Some(idx) = line.find(arrow) {
            let from = line[..idx].trim().to_string();
            let rest = &line[idx + arrow.len()..];
            let (to, label) = match rest.split_once(':') {
                Some((t, l)) => (t.trim().to_string(), l.trim().to_string()),
                None => (rest.trim().to_string(), String::new()),
            };
            if from.is_empty() || to.is_empty() {
                continue;
            }
            return Some((from, to, label));
        }
    }
    None
}

pub fn parse(src: &str) -> Result<Diagram, Fallback> {
    let mut actors: Vec<String> = Vec::new();
    let mut messages: Vec<Message> = Vec::new();
    let mut id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    let mut lines = src
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    lines.next();

    for line in lines {
        if let Some((id, name)) = parse_participant_line(line) {
            ensure_actor(&mut actors, &name);
            id_map.insert(id, name);
            continue;
        }

        if let Some((from_raw, to_raw, label)) = parse_message_line(line) {
            let from = id_map.get(&from_raw).cloned().unwrap_or(from_raw);
            let to = id_map.get(&to_raw).cloned().unwrap_or(to_raw);
            ensure_actor(&mut actors, &from);
            ensure_actor(&mut actors, &to);
            let self_msg = from == to;
            messages.push(Message { from, to, label, self_msg });
        }
    }

    if actors.is_empty() && messages.is_empty() {
        return Err(Fallback::Empty { kind: "sequence".to_string() });
    }

    Ok(Diagram::Sequence { actors, messages })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_participants_and_alias() {
        let src = "sequenceDiagram\n  participant A\n  participant B as Bob";
        let res = parse(src).unwrap();
        if let Diagram::Sequence { actors, .. } = res {
            assert_eq!(actors, vec!["A".to_string(), "Bob".to_string()]);
        } else {
            panic!("not a sequence");
        }
    }

    #[test]
    fn test_implicit_participant_from_message() {
        let src = "sequenceDiagram\n  Alice->>Bob: Hello";
        let res = parse(src).unwrap();
        if let Diagram::Sequence { actors, messages } = res {
            assert_eq!(actors, vec!["Alice".to_string(), "Bob".to_string()]);
            assert_eq!(messages[0].from, "Alice");
            assert_eq!(messages[0].to, "Bob");
            assert_eq!(messages[0].label, "Hello");
            assert!(!messages[0].self_msg);
        } else {
            panic!("not a sequence");
        }
    }

    #[test]
    fn test_all_message_arrow_forms() {
        let src = "sequenceDiagram\n  A->>B: m1\n  A-->>B: m2\n  A->B: m3\n  A-->B: m4";
        let res = parse(src).unwrap();
        if let Diagram::Sequence { messages, .. } = res {
            assert_eq!(messages.len(), 4);
            for m in &messages {
                assert_eq!(m.from, "A");
                assert_eq!(m.to, "B");
            }
        } else {
            panic!("not a sequence");
        }
    }

    #[test]
    fn test_self_message() {
        let src = "sequenceDiagram\n  A->>A: think";
        let res = parse(src).unwrap();
        if let Diagram::Sequence { messages, .. } = res {
            assert!(messages[0].self_msg);
        } else {
            panic!("not a sequence");
        }
    }

    #[test]
    fn test_aliased_participant_id_resolves_to_display_name() {
        let src = "sequenceDiagram\n  participant U as User\n  participant A as App\n  U->>A: hi";
        let res = parse(src).unwrap();
        if let Diagram::Sequence { actors, messages } = res {
            assert_eq!(actors, vec!["User".to_string(), "App".to_string()]);
            assert_eq!(messages[0].from, "User");
            assert_eq!(messages[0].to, "App");
            assert!(!messages[0].self_msg);
        } else {
            panic!("not a sequence");
        }
    }

    #[test]
    fn test_zero_content_is_empty_fallback() {
        let src = "sequenceDiagram\n  %% nothing";
        let res = parse(src);
        assert!(matches!(res, Err(Fallback::Empty { .. })));
    }
}
