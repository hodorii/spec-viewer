pub mod class;
pub mod er;
pub mod flow;
pub mod generic;
pub mod seq;
pub mod state;

use super::Fallback;

#[derive(Debug, PartialEq)]
pub enum Dir { TB, LR }

#[derive(Debug, PartialEq)]
pub enum Shape {
    Box(String),
    Entity { name: String, fields: Vec<String> },
    Class { name: String, attrs: Vec<String>, methods: Vec<String> },
    State(String),
    Start,
    End,
}

#[derive(Debug, PartialEq)]
pub struct Node {
    pub id: String,
    pub shape: Shape,
}

#[derive(Debug, PartialEq)]
pub enum EdgeStyle {
    Arrow,
    Line,
    Cardinality(String, String),
    Inherit,
    Compose,
    Aggregate,
    Transition,
}

#[derive(Debug, PartialEq)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
    pub style: EdgeStyle,
}

#[derive(Debug, PartialEq)]
pub struct Subgraph {
    pub title: String,
    pub members: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub struct Message {
    pub from: String,
    pub to: String,
    pub label: String,
    pub self_msg: bool,
}

#[derive(Debug, PartialEq)]
pub enum Diagram {
    Graph {
        dir: Dir,
        nodes: Vec<Node>,
        edges: Vec<Edge>,
        groups: Vec<Subgraph>,
    },
    Sequence {
        actors: Vec<String>,
        messages: Vec<Message>,
    },
}

pub fn parse(src: &str) -> Result<Diagram, Fallback> {
    let first_line = src.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .ok_or_else(|| Fallback::Empty { kind: "unknown".to_string() })?;

    if first_line.starts_with("graph") || first_line.starts_with("flowchart") {
        flow::parse(src)
    } else if first_line.starts_with("erDiagram") {
        er::parse(src)
    } else if first_line.starts_with("classDiagram") {
        class::parse(src)
    } else if first_line.starts_with("stateDiagram") {
        state::parse(src)
    } else if first_line.starts_with("sequenceDiagram") {
        seq::parse(src)
    } else {
        generic::parse(src)
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;

    #[test]
    fn test_dispatch_flow() {
        let res = parse("graph LR\n  A --> B").unwrap();
        assert!(matches!(res, Diagram::Graph { .. }));
    }

    #[test]
    fn test_dispatch_er() {
        let res = parse("erDiagram\n  CUSTOMER ||--o{ ORDER : places").unwrap();
        assert!(matches!(res, Diagram::Graph { .. }));
    }

    #[test]
    fn test_dispatch_class() {
        let res = parse("classDiagram\n  class Foo { +int x }").unwrap();
        assert!(matches!(res, Diagram::Graph { .. }));
    }

    #[test]
    fn test_dispatch_state() {
        let res = parse("stateDiagram-v2\n  [*] --> Idle").unwrap();
        assert!(matches!(res, Diagram::Graph { .. }));
    }

    #[test]
    fn test_dispatch_sequence() {
        let res = parse("sequenceDiagram\n  Alice->>Bob: Hi").unwrap();
        assert!(matches!(res, Diagram::Sequence { .. }));
    }

    #[test]
    fn test_dispatch_generic_for_unsupported_kind() {
        let res = parse("gantt\n  Task1 done").unwrap();
        assert!(matches!(res, Diagram::Graph { .. }));
    }
}
