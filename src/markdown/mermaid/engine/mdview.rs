//! The `mdview` [`GraphEngine`](super::GraphEngine) — a layered (Sugiyama-style)
//! graph layout adopted from `~/dev/tools/mdview`'s
//! `src/render/mermaid/graph.rs` (MIT — see `THIRD_PARTY.md`), replacing the
//! builtin engine's shared-vertical-bus wiring (which merges unrelated edges
//! into a single `┼` column and loses start/end traceability — requirement
//! 5.20, `research.md` "그래프 배선 — mdview 엔진 채택").
//!
//! ## What changed from mdview's original
//! - No `Theme`/`Style`/styled `Line` — this crate's mermaid output is plain
//!   `Vec<String>` (color, if any, is applied once at `SpanStyle::Code` by
//!   `code.rs`), so every `Style` parameter is dropped and `Canvas` is the
//!   unstyled one already ported for task 9.5 (`super::super::canvas`).
//! - `Shape`/`Arrow` renamed `MShape`/`MArrow` (this crate's `parse::Shape`
//!   already owns that name).
//! - **LR (5.7, revised 16.3)**: mdview's engine is TB-only by design (its
//!   own comment: "터미널은 세로로 길기 때문에 방향 지정과 무관하게 항상
//!   위에서 아래로 배치한다") — this port keeps that unconditionally,
//!   matching the reference binary's output exactly regardless of a `graph
//!   LR`/`flowchart LR` declaration (declared direction is otherwise
//!   ignored here). An earlier revision of this port added real LR support
//!   by transposing the rank/order axes; it was removed (16.3) after a real
//!   side-by-side comparison against the reference `mdview` binary showed
//!   the transposed connector drawing producing corner-glyph overlaps and
//!   stray `┼` crossings that the original TB-only wiring does not have.
//!   `--diagram-engine builtin` still renders LR (5.7's LR requirement is
//!   met by that engine).
//! - Fallback: mdview always renders (shrinking labels down to a `cap` of 9
//!   and accepting overflow at that point); this port keeps that shrink
//!   loop but returns `Fallback::Overflow` instead of an oversized render if
//!   even `cap == 9` doesn't fit `width`, matching this crate's existing
//!   engine-fallback convention (`code.rs`'s mermaid handling).
//!
//! Start/End (mermaid `[*]`) nodes get a small bordered box around `●`/`◉`
//! here (mdview's `sizes()` always pads +4/+2 for a border) rather than the
//! builtin engine's borderless bare glyph — a deliberate, disclosed cosmetic
//! difference, not a defect (5.19 only requires a recognizable start/end
//! mark).

use super::super::canvas::Canvas;
use super::super::parse::{Diagram, EdgeStyle as PEdgeStyle, Shape as PShape};
use super::super::Fallback;
use super::GraphEngine;
use std::collections::HashMap;

// ---------------------------------------------------------------- data model

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MShape {
    Rect,
    Round,
    Diamond,
    Cylinder,
    #[allow(dead_code)]
    Subroutine,
    Circle,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MArrow {
    None,
    Open,
    Hollow,
    Diamond,
    HollowDiamond,
    #[allow(dead_code)]
    Cross,
}

const DIVIDER: &str = "\u{1}";

#[derive(Clone, Debug)]
struct GNode {
    lines: Vec<String>,
    shape: MShape,
    group: Option<usize>,
    left_align: bool,
    header: bool,
}

#[derive(Clone, Debug)]
struct GEdge {
    from: usize,
    to: usize,
    label: String,
    head: MArrow,
    tail: MArrow,
    head_tag: String,
    tail_tag: String,
}

#[derive(Clone, Debug)]
struct MGroup {
    title: String,
    members: Vec<usize>,
}

#[derive(Default, Clone)]
struct Graph {
    nodes: Vec<GNode>,
    edges: Vec<GEdge>,
    groups: Vec<MGroup>,
}

// ---------------------------------------------------------------- adapter

fn shape_lines(shape: &PShape) -> (Vec<String>, MShape, bool, bool) {
    match shape {
        PShape::Box(label) => (vec![label.clone()], MShape::Rect, false, false),
        PShape::State(name) => (vec![name.clone()], MShape::Round, false, false),
        PShape::Start => (vec!["\u{25cf}".to_string()], MShape::Circle, false, false), // ●
        PShape::End => (vec!["\u{25c9}".to_string()], MShape::Circle, false, false),   // ◉
        PShape::Entity { name, fields } => {
            let mut lines = vec![name.clone(), DIVIDER.to_string()];
            lines.extend(fields.iter().cloned());
            (lines, MShape::Rect, true, true)
        }
        PShape::Class { name, attrs, methods } => {
            let mut lines = vec![name.clone(), DIVIDER.to_string()];
            lines.extend(attrs.iter().cloned());
            lines.push(DIVIDER.to_string());
            lines.extend(methods.iter().cloned());
            (lines, MShape::Rect, true, true)
        }
    }
}

/// Returns `(tail, head, tail_tag, head_tag)` — `tail` is the arrowhead at
/// the edge's `from` end, `head` at its `to` end. Matches the builtin
/// engine's authoritative `glyph_ends` convention (`graph.rs`) exactly, so
/// switching engines doesn't flip which end of a relationship gets the
/// arrow/diamond: plain `-->`/state `Transition` arrows point at `to` only;
/// `Cardinality(l, r)` puts `l` at `from`, `r` at `to` (matches `er.rs`'s
/// `Cardinality(left_card, right_card)`); `Inherit` (`<|--`/`--|>`, both
/// normalized to `to` by the class parser) puts the hollow triangle at
/// `to`; `Compose`/`Aggregate` put the filled/hollow diamond at `from` (the
/// owning class is always the parser's `left`/`from` side).
fn edge_style(style: &PEdgeStyle) -> (MArrow, MArrow, String, String) {
    match style {
        PEdgeStyle::Arrow => (MArrow::None, MArrow::Open, String::new(), String::new()),
        PEdgeStyle::Line => (MArrow::None, MArrow::None, String::new(), String::new()),
        PEdgeStyle::Cardinality(tail_tag, head_tag) => {
            (MArrow::None, MArrow::None, tail_tag.clone(), head_tag.clone())
        }
        PEdgeStyle::Inherit => (MArrow::None, MArrow::Hollow, String::new(), String::new()),
        PEdgeStyle::Compose => (MArrow::Diamond, MArrow::None, String::new(), String::new()),
        PEdgeStyle::Aggregate => (MArrow::HollowDiamond, MArrow::None, String::new(), String::new()),
        PEdgeStyle::Transition => (MArrow::None, MArrow::Open, String::new(), String::new()),
    }
}

/// Maps this crate's parser output (`Diagram::Graph`) onto the engine's own
/// node-index-based model. Edges whose endpoint id isn't a known node are
/// dropped (matches the builtin engine's `graph::layout` `filter_map`).
fn build_graph(diagram: &Diagram) -> Option<Graph> {
    // 16.3: 방향(dir)은 더 이상 쓰지 않는다 — mdview 원본처럼 항상 TB.
    let Diagram::Graph { dir: _, nodes, edges, groups } = diagram else { return None };
    let index_of: HashMap<&str, usize> =
        nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();

    let mut gnodes: Vec<GNode> = nodes
        .iter()
        .map(|n| {
            let (lines, shape, left_align, header) = shape_lines(&n.shape);
            GNode { lines, shape, group: None, left_align, header }
        })
        .collect();

    let mgroups: Vec<MGroup> = groups
        .iter()
        .map(|g| MGroup {
            title: g.title.clone(),
            members: g.members.iter().filter_map(|id| index_of.get(id.as_str()).copied()).collect(),
        })
        .collect();
    for (gi, g) in mgroups.iter().enumerate() {
        for &m in &g.members {
            gnodes[m].group = Some(gi);
        }
    }

    let gedges: Vec<GEdge> = edges
        .iter()
        .filter_map(|e| {
            let from = *index_of.get(e.from.as_str())?;
            let to = *index_of.get(e.to.as_str())?;
            let (tail, head, tail_tag, head_tag) = edge_style(&e.style);
            Some(GEdge {
                from,
                to,
                label: e.label.clone().unwrap_or_default(),
                head,
                tail,
                head_tag,
                tail_tag,
            })
        })
        .collect();

    Some(Graph {
        nodes: gnodes,
        edges: gedges,
        groups: mgroups,
    })
}

// ---------------------------------------------------------------- placement

#[derive(Clone, Debug)]
struct Placed {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    rank: usize,
    dummy: bool,
    node: usize,
}

impl Placed {
    /// Order-axis coordinate/size — always real-x/real-w (16.3: TB-only, no
    /// more LR transpose).
    fn u(&self) -> usize {
        self.x
    }
    fn v(&self) -> usize {
        self.y
    }
    fn pu(&self) -> usize {
        self.w
    }
}

#[derive(Clone, Copy)]
struct Seg {
    edge: usize,
    top: usize,
    bottom: usize,
}

struct Plan {
    placed: Vec<Placed>,
    segments: Vec<Seg>,
    seg_row: Vec<usize>,
    band_rows: Vec<usize>,
    band_start: Vec<usize>,
    gutters: Vec<(usize, usize)>,
    fb_row: HashMap<usize, usize>,
    selfloop: Vec<usize>,
    total_w: usize,
    total_h: usize,
    lines: Vec<Vec<String>>,
}

const HGAP: usize = 3;
const GROUP_PAD: usize = 2;

impl Graph {
    fn render(&self, width: usize) -> Result<Vec<String>, Fallback> {
        // 16.1: 5.16 사다리에 '본문 접기' 단계 추가 — 라벨 축약(cap 30→9)을
        // 모두 시도해도 넘치면, Class/Entity 상자를 속성/메서드를 버리고
        // 이름만 남긴 상자로 접어 같은 cap 사다리를 다시 탄다(실측: cap
        // 만으로는 sizes() 의 `cap.max(22)` 때문에 본문이 22열 아래로 안
        // 줄어든다 — gitea-github-schema.md 가 100/120열에서 그래서 못
        // 들어갔다). 이 사다리는 `place(..., wrap: false)` 로만 시도한다 —
        // 리뷰 실측 회귀: `wrap_rows` 를 모든 rung 에 무조건 적용했더니
        // Boundary Map(flowchart, 헤더 없음) 이 폭 40 골든에서 원래
        // Overflow 여야 할 것이 래핑으로 그냥 들어가 버려 골든이 깨졌다.
        for fold in [false, true] {
            for cap in [30usize, 24, 20, 16, 12, 9] {
                let boxes = self.sizes(cap, fold);
                let plan = self.place(&boxes, width, false);
                if plan.total_w <= width {
                    return Ok(self.draw(&plan));
                }
            }
        }
        // 16.1 마지막 rung: '층 내 행 래핑' — Class/Entity 본문이 있는
        // 다이어그램에서만, 가장 압축된 상태(cap=9, 본문 접기)로도 안
        // 들어가면 그 상태 그대로 한 층을 여러 줄로 나눠 마지막으로 한 번
        // 더 시도한다. `n.header`(Class/Entity 전용) 로 한정해 flowchart 등
        // 이미 Overflow 로 확정된 기존 골든(design.md/합성 픽스처의
        // Boundary Map, 폭 40)의 동작을 절대 바꾸지 않는다.
        if self.nodes.iter().any(|n| n.header) {
            let boxes = self.sizes(9, true);
            let plan = self.place(&boxes, width, true);
            if plan.total_w <= width {
                return Ok(self.draw(&plan));
            }
        }
        Err(Fallback::Overflow { kind: "flowchart".to_string() })
    }

    /// `fold`: Class/Entity 상자의 속성/메서드(첫 줄 이후 전부)를 버리고
    /// 이름만 남긴 상자로 그린다(16.1 사다리의 마지막 rung — `cap.max(22)`
    /// 때문에 라벨 축약만으로는 줄지 않는 본문을 완전히 접어낸다).
    fn sizes(&self, cap: usize, fold: bool) -> Vec<(usize, usize, Vec<String>)> {
        self.nodes
            .iter()
            .map(|n| {
                let mut lines: Vec<String> = Vec::new();
                if fold && n.header {
                    if let Some(name) = n.lines.first() {
                        lines.extend(wrap_label(name, cap));
                    }
                } else {
                    for raw in &n.lines {
                        if raw == DIVIDER {
                            lines.push(DIVIDER.to_string());
                        } else {
                            lines.extend(wrap_label(raw, cap.max(if n.left_align { 22 } else { 0 })));
                        }
                    }
                }
                if lines.is_empty() {
                    lines.push(String::new());
                }
                let w = lines.iter().filter(|l| *l != DIVIDER).map(|l| width_of(l)).max().unwrap_or(0) + 4;
                let h = lines.len() + 2;
                (w.max(5), h, lines)
            })
            .collect()
    }

    fn rank_nodes(&self) -> Vec<usize> {
        let n = self.nodes.len();
        let ng = self.groups.len();
        let cluster: Vec<usize> = (0..n).map(|i| self.nodes[i].group.unwrap_or(ng + i)).collect();
        let ncl = ng + n;

        let intra: Vec<(usize, usize)> = self
            .edges
            .iter()
            .filter(|e| e.from != e.to && cluster[e.from] == cluster[e.to])
            .map(|e| (e.from, e.to))
            .collect();
        let local = longest_path(n, &intra);
        let mut height = vec![1usize; ncl];
        for i in 0..n {
            height[cluster[i]] = height[cluster[i]].max(local[i] + 1);
        }

        let inter: Vec<(usize, usize)> = self
            .edges
            .iter()
            .filter(|e| cluster[e.from] != cluster[e.to])
            .map(|e| (cluster[e.from], cluster[e.to]))
            .collect();
        let crank = longest_path(ncl, &inter);

        let mut order: Vec<usize> = (0..ncl).collect();
        order.sort_by_key(|c| crank[*c]);
        let mut base = vec![0usize; ncl];
        for &c in &order {
            for &(a, b) in &inter {
                if b == c && crank[a] < crank[c] {
                    base[c] = base[c].max(base[a] + height[a]);
                }
            }
        }
        (0..n).map(|i| base[cluster[i]] + local[i]).collect()
    }

    fn order_ranks(&self, rank: &[usize], nranks: usize) -> Vec<Vec<usize>> {
        let mut ranks: Vec<Vec<usize>> = vec![Vec::new(); nranks];
        for (i, r) in rank.iter().enumerate() {
            ranks[*r].push(i);
        }
        let mut pos: Vec<usize> = vec![0; self.nodes.len()];
        for r in &ranks {
            for (i, v) in r.iter().enumerate() {
                pos[*v] = i;
            }
        }
        for pass in 0..4 {
            let down = pass % 2 == 0;
            let order: Vec<usize> = if down { (1..nranks).collect() } else { (0..nranks.saturating_sub(1)).rev().collect() };
            for r in order {
                let mut keyed: Vec<(f32, usize, usize)> = ranks[r]
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| {
                        let mut sum = 0f32;
                        let mut cnt = 0f32;
                        for e in &self.edges {
                            let other = if e.to == v && down {
                                Some(e.from)
                            } else if e.from == v && !down {
                                Some(e.to)
                            } else {
                                None
                            };
                            if let Some(o) = other {
                                if rank[o] != rank[v] {
                                    sum += pos[o] as f32;
                                    cnt += 1.0;
                                }
                            }
                        }
                        let bc = if cnt > 0.0 { sum / cnt } else { i as f32 };
                        (bc, i, v)
                    })
                    .collect();
                keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
                ranks[r] = keyed.iter().map(|k| k.2).collect();
                for (i, v) in ranks[r].iter().enumerate() {
                    pos[*v] = i;
                }
            }
        }
        for r in ranks.iter_mut() {
            let mut mean: HashMap<usize, (f32, f32)> = HashMap::new();
            for (i, v) in r.iter().enumerate() {
                if let Some(g) = self.nodes.get(*v).and_then(|n| n.group) {
                    let e = mean.entry(g).or_insert((0.0, 0.0));
                    e.0 += i as f32;
                    e.1 += 1.0;
                }
            }
            let mut keyed: Vec<(f32, usize, usize)> = r
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let key = match self.nodes.get(v).and_then(|n| n.group) {
                        Some(g) => mean.get(&g).map(|(s, c)| s / c).unwrap_or(i as f32),
                        None => i as f32,
                    };
                    (key, i, v)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
            *r = keyed.iter().map(|k| k.2).collect();
        }
        ranks
    }

    #[allow(clippy::needless_range_loop)]
    fn place(&self, boxes: &[(usize, usize, Vec<String>)], width: usize, wrap: bool) -> Plan {
        let n = self.nodes.len();
        let rank = self.rank_nodes();
        let nranks = rank.iter().copied().max().map(|m| m + 1).unwrap_or(1);
        let ranks = self.order_ranks(&rank, nranks);

        // 16.3: 항상 TB — 순서축 크기는 너비, 층축 크기는 높이.
        let order_size = |i: usize| -> usize { boxes[i].0 };
        let rank_size = |i: usize| -> usize { boxes[i].1 };

        // 16.1: 층 내 행 래핑 — `wrap` 일 때만(사다리의 마지막 rung, 호출부
        // `render()` 참고) 한 층(rank)의 노드를 순서를 유지한 채 폭
        // 예산(`width`)에 맞춰 여러 '줄'(하위 rank)로 쪼갠다. 이후 파이프
        // 라인은 원래도 임의의 rank 간격(gap>1)에 대해 더미 노드 체인으로
        // 배선하므로(아래 세그먼트 계산부), 새로 쪼갠 경계를 넘는 간선도
        // 같은 체인 로직이 자동으로 '행 사이 통로'를 만든다 — 별도 배선
        // 코드가 필요 없다. `wrap == false` 일 때는 `ranks`/`rank`/`nranks`
        // 가 16.1 이전과 완전히 동일해 어떤 회귀도 없다(리뷰 실측: 이전엔
        // 모든 rung 에서 무조건 래핑을 시도해 회귀가 났었다).
        let (rank, ranks, nranks) = if wrap {
            let wrapped = wrap_rows(&ranks, order_size, width);
            let mut new_rank = vec![0usize; n];
            for (r, ids) in wrapped.iter().enumerate() {
                for &i in ids {
                    new_rank[i] = r;
                }
            }
            let new_nranks = wrapped.len().max(1);
            (new_rank, wrapped, new_nranks)
        } else {
            (rank, ranks, nranks)
        };

        let mut ranks: Vec<Vec<usize>> = ranks;
        let mut dummy_rank: Vec<usize> = Vec::new();
        let mut segments: Vec<Seg> = Vec::new();
        let mut feedback: Vec<usize> = Vec::new();
        let mut selfloop: Vec<usize> = Vec::new();
        for (ei, e) in self.edges.iter().enumerate() {
            if e.from == e.to {
                selfloop.push(ei);
                continue;
            }
            let (ra, rb) = (rank[e.from], rank[e.to]);
            if ra >= rb {
                feedback.push(ei);
                continue;
            }
            let (a, b) = (e.from, e.to);
            let mut chain: Vec<usize> = Vec::new();
            if rb - ra > 1 {
                for (r, rank_ids) in ranks.iter_mut().enumerate().take(rb).skip(ra + 1) {
                    let id = n + dummy_rank.len();
                    dummy_rank.push(r);
                    rank_ids.push(id);
                    chain.push(id);
                }
            }
            let mut prev = a;
            for &d in &chain {
                segments.push(Seg { edge: ei, top: prev, bottom: d });
                prev = d;
            }
            segments.push(Seg { edge: ei, top: prev, bottom: b });
        }

        let total = n + dummy_rank.len();
        let mut pu: Vec<usize> = vec![0usize; total]; // 순서축 좌표(패킹 결과)
        let mut psz: Vec<usize> = vec![1usize; total]; // 순서축 크기(패킹에 쓰는 폭/높이)
        for i in 0..n {
            psz[i] = order_size(i);
        }
        for &d in &dummy_rank {
            let _ = d; // 가상 노드는 이미 psz 기본값 1
        }

        let mut rank_extent = vec![1usize; nranks];
        for (r, ids) in ranks.iter().enumerate() {
            rank_extent[r] = ids.iter().map(|&i| if i < n { rank_size(i) } else { 1 }).max().unwrap_or(1);
        }

        let ng = self.groups.len();
        let item_of: Vec<usize> = (0..total)
            .map(|i| match self.nodes.get(i).and_then(|nd| nd.group) {
                Some(g) => g,
                None => ng + i,
            })
            .collect();
        let nitems = ng + total;
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); nitems];
        for i in 0..total {
            members[item_of[i]].push(i);
        }

        for it in 0..nitems {
            if members[it].len() <= 1 {
                if let Some(&i) = members[it].first() {
                    pu[i] = 0;
                }
                continue;
            }
            let sub: Vec<Vec<usize>> = ranks.iter().map(|ids| ids.iter().copied().filter(|&i| item_of[i] == it).collect()).collect();
            for ids in &sub {
                let mut x = 0;
                for (k, &i) in ids.iter().enumerate() {
                    if k > 0 {
                        x += HGAP;
                    }
                    pu[i] = x;
                    x += psz[i];
                }
            }
            let inner: Vec<Seg> = segments.iter().copied().filter(|sg| item_of[sg.top] == it && item_of[sg.bottom] == it).collect();
            for _ in 0..3 {
                for r in 1..nranks {
                    self.align_rank(&sub, r, &inner, &mut pu, &psz, true);
                }
                for r in (0..nranks.saturating_sub(1)).rev() {
                    self.align_rank(&sub, r, &inner, &mut pu, &psz, false);
                }
            }
            let base = members[it].iter().map(|&i| pu[i]).min().unwrap_or(0);
            for &i in &members[it] {
                pu[i] -= base;
            }
        }
        let item_w: Vec<usize> = (0..nitems).map(|it| members[it].iter().map(|&i| pu[i] + psz[i]).max().unwrap_or(0)).collect();
        let item_span: Vec<Option<(usize, usize)>> = (0..nitems)
            .map(|it| {
                let rs: Vec<usize> = members[it].iter().map(|&i| node_rank(i, n, &rank, &dummy_rank)).collect();
                rs.iter().copied().min().map(|lo| (lo, rs.iter().copied().max().unwrap_or(lo)))
            })
            .collect();

        let mut item_x: Vec<Option<usize>> = vec![None; nitems];
        let mut cursor = vec![0usize; nranks];
        for r in 0..nranks {
            let mut seen: Vec<usize> = Vec::new();
            for &i in &ranks[r] {
                let it = item_of[i];
                if !seen.contains(&it) {
                    seen.push(it);
                }
            }
            for it in seen {
                if item_x[it].is_some() {
                    continue;
                }
                let Some((lo, hi)) = item_span[it] else { continue };
                let grouped = it < ng;
                let pad = if grouped { GROUP_PAD } else { 0 };
                let off = (lo..=hi).map(|r2| cursor[r2]).max().unwrap_or(0) + pad;
                item_x[it] = Some(off);
                for c in cursor.iter_mut().take(hi + 1).skip(lo) {
                    *c = off + item_w[it] + pad + HGAP;
                }
            }
        }
        for i in 0..total {
            pu[i] += item_x[item_of[i]].unwrap_or(0);
        }

        let body_extent = (0..n).map(|i| pu[i] + psz[i]).max().unwrap_or(0) + 1;
        let gutters: Vec<(usize, usize)> = feedback.iter().enumerate().map(|(i, &ei)| (ei, body_extent + 1 + i * 2)).collect();

        let mut band_rows = vec![1usize; nranks];
        let mut seg_row: Vec<usize> = vec![0; segments.len()];
        let mut fb_row: HashMap<usize, usize> = HashMap::new();
        for r in 0..nranks {
            let mut intervals: Vec<(usize, usize, usize)> = Vec::new();
            for (si, seg) in segments.iter().enumerate() {
                let (ei, a, b) = (seg.edge, seg.top, seg.bottom);
                let ra = node_rank(a, n, &rank, &dummy_rank);
                if ra != r {
                    continue;
                }
                let (ax, bx) = (center(pu[a], psz[a]), center(pu[b], psz[b]));
                let lab = width_of(&self.edges[ei].label);
                let lo = ax.min(bx);
                let hi = ax.max(bx) + if lab > 0 { lab + 2 } else { 0 };
                intervals.push((lo, hi, si));
            }
            for &(ei, gx) in &gutters {
                let src = self.edges[ei].from;
                if rank[src] != r {
                    continue;
                }
                let lo = center(pu[src], psz[src]);
                let lab = width_of(&self.edges[ei].label);
                intervals.push((lo, gx.max(lo + lab + 2), usize::MAX - ei));
            }
            intervals.sort_by_key(|t| t.0);
            let mut ends: Vec<usize> = Vec::new();
            for (lo, hi, si) in intervals {
                let slot = ends.iter().position(|&e| e + 1 < lo).unwrap_or(ends.len());
                if slot == ends.len() {
                    ends.push(hi);
                } else {
                    ends[slot] = hi;
                }
                if si > segments.len() {
                    fb_row.insert(usize::MAX - si, slot);
                } else {
                    seg_row[si] = slot;
                }
            }
            band_rows[r] = ends.len().max(1);
        }

        let group_ranks = self.group_rank_spans(&ranks, n);
        let mut rank_v: Vec<usize> = vec![0; nranks];
        let mut band_start = vec![0usize; nranks];
        let mut vv = 0usize;
        for r in 0..nranks {
            if group_ranks.iter().any(|(_, lo, _)| *lo == r) {
                vv += 2;
            }
            rank_v[r] = vv;
            vv += rank_extent[r];
            if group_ranks.iter().any(|(_, _, hi)| *hi == r) {
                vv += 1;
            }
            band_start[r] = vv;
            if r + 1 < nranks {
                vv += band_rows[r] + 2;
            }
        }
        let total_v = vv;

        let mut placed: Vec<Placed> = Vec::with_capacity(total);
        for i in 0..total {
            let r = node_rank(i, n, &rank, &dummy_rank);
            let sz = if i < n { rank_size(i) } else { rank_extent[r] };
            // (u, v) 는 알고리즘 자신의 축(순서, 층) — TB 에서는 그대로 x/y.
            let (real_x, real_y, real_w, real_h) = (pu[i], rank_v[r], psz[i], sz);
            // 실제 박스 폭/높이는 절대 뒤집지 않는다 — sizes() 가 낸 실측값 그대로.
            let (real_w, real_h) = if i < n { (boxes[i].0, boxes[i].1) } else { (real_w, real_h) };
            placed.push(Placed { x: real_x, y: real_y, w: real_w, h: real_h, rank: r, dummy: i >= n, node: i });
        }

        self.separate_groups(&mut placed, n);
        self.compact_x(&mut placed);

        let minu = placed.iter().map(|p| p.u()).min().unwrap_or(0);
        let pad = if self.groups.is_empty() { 0 } else { 2 };
        for p in placed.iter_mut() {
            p.x = p.u() - minu + pad;
        }
        let body_extent = placed.iter().filter(|p| !p.dummy).map(|p| p.u() + p.pu()).max().unwrap_or(0) + 2;
        let gutters: Vec<(usize, usize)> = gutters.iter().enumerate().map(|(i, (ei, _))| (*ei, body_extent + i * 2)).collect();
        let total_u = gutters.last().map(|(_, g)| g + 2).unwrap_or(body_extent) + GROUP_PAD;

        let (total_w, total_h) = (total_u, total_v + 1);

        Plan {
            placed,
            segments,
            seg_row,
            band_rows,
            band_start,
            gutters,
            fb_row,
            selfloop,
            total_w,
            total_h,
            lines: boxes.iter().map(|b| b.2.clone()).collect(),
        }
    }

    fn align_rank(&self, ranks: &[Vec<usize>], r: usize, segments: &[Seg], pu: &mut [usize], psz: &[usize], down: bool) {
        let ids = &ranks[r];
        let mut desired: Vec<(usize, usize)> = Vec::with_capacity(ids.len());
        for &i in ids {
            let mut sum = 0usize;
            let mut cnt = 0usize;
            for seg in segments {
                let (a, b) = (seg.top, seg.bottom);
                let other = if down && b == i {
                    Some(a)
                } else if !down && a == i {
                    Some(b)
                } else {
                    None
                };
                if let Some(o) = other {
                    sum += center(pu[o], psz[o]);
                    cnt += 1;
                }
            }
            let want = match sum.checked_div(cnt) {
                Some(avg) => avg.saturating_sub(psz[i] / 2),
                None => pu[i],
            };
            desired.push((i, want));
        }
        let mut cursor = 0usize;
        let mut first = true;
        for (i, want) in desired {
            let x = if first { want } else { want.max(cursor) };
            pu[i] = x;
            cursor = x + psz[i] + HGAP;
            first = false;
        }
    }

    fn compact_x(&self, placed: &mut [Placed]) {
        const MAX_GAP: usize = 5;
        let maxu = placed.iter().map(|p| p.u() + p.pu()).max().unwrap_or(0);
        let mut occ = vec![false; maxu + 2];
        for p in placed.iter() {
            for slot in &mut occ[p.u()..p.u() + p.pu()] {
                *slot = true;
            }
        }
        for g in &self.groups {
            let ms: Vec<&Placed> = placed.iter().filter(|p| !p.dummy && g.members.contains(&p.node)).collect();
            if ms.is_empty() {
                continue;
            }
            let a = ms.iter().map(|p| p.u()).min().unwrap_or(0).saturating_sub(2);
            let b = (ms.iter().map(|p| p.u() + p.pu()).max().unwrap_or(0) + 1).min(maxu);
            for slot in &mut occ[a..=b] {
                *slot = true;
            }
        }
        let mut map = vec![0usize; maxu + 2];
        let mut cur = 0usize;
        let mut run = 0usize;
        for x in 0..=maxu + 1 {
            map[x] = cur;
            if x <= maxu && occ[x] {
                cur += 1;
                run = 0;
            } else {
                run += 1;
                if run <= MAX_GAP {
                    cur += 1;
                }
            }
        }
        for p in placed.iter_mut() {
            p.x = map[p.u()];
        }
    }

    fn separate_groups(&self, placed: &mut [Placed], n: usize) {
        if self.groups.len() < 2 {
            return;
        }
        for _ in 0..self.groups.len() {
            let boxes: Vec<Option<(usize, usize, usize, usize)>> = self
                .groups
                .iter()
                .map(|g| {
                    let ms: Vec<&Placed> = placed.iter().filter(|p| !p.dummy && g.members.contains(&p.node)).collect();
                    if ms.is_empty() {
                        return None;
                    }
                    Some((
                        ms.iter().map(|p| p.u()).min().unwrap_or(0).saturating_sub(2),
                        ms.iter().map(|p| p.u() + p.pu()).max().unwrap_or(0) + 1,
                        ms.iter().map(|p| p.v()).min().unwrap_or(0).saturating_sub(2),
                        ms.iter().map(|p| p.v() + p.h).max().unwrap_or(0),
                    ))
                })
                .collect();
            let mut shift: Option<(usize, usize)> = None;
            'outer: for a in 0..boxes.len() {
                for b in 0..boxes.len() {
                    let (Some(ba), Some(bb)) = (boxes[a], boxes[b]) else { continue };
                    if a == b || ba.0 > bb.0 {
                        continue;
                    }
                    let overlap_y = ba.2 < bb.3 && bb.2 < ba.3;
                    let overlap_x = ba.0 < bb.1 && bb.0 < ba.1;
                    if overlap_y && overlap_x {
                        shift = Some((bb.0, ba.1 + 2 - bb.0));
                        break 'outer;
                    }
                }
            }
            let Some((from_u, delta)) = shift else { return };
            for p in placed.iter_mut() {
                if p.u() >= from_u && (p.dummy || p.node < n) {
                    p.x += delta;
                }
            }
        }
    }

    fn group_rank_spans(&self, ranks: &[Vec<usize>], n: usize) -> Vec<(usize, usize, usize)> {
        let mut out = Vec::new();
        for (gi, g) in self.groups.iter().enumerate() {
            let mut lo = usize::MAX;
            let mut hi = 0usize;
            for (r, ids) in ranks.iter().enumerate() {
                if ids.iter().any(|&i| i < n && g.members.contains(&i)) {
                    lo = lo.min(r);
                    hi = hi.max(r);
                }
            }
            if lo != usize::MAX {
                out.push((gi, lo, hi));
            }
        }
        out
    }

    fn draw(&self, plan: &Plan) -> Vec<String> {
        let mut c = Canvas::new(plan.total_w + 2, plan.total_h + 2);

        // 1) 서브그래프 테두리 — 박스는 늘 실제 x/y/w/h 로, 방향과 무관.
        let mut group_titles: Vec<(usize, usize, String)> = Vec::new();
        for g in self.groups.iter() {
            let members: Vec<&Placed> = plan.placed.iter().filter(|p| !p.dummy && g.members.contains(&p.node)).collect();
            if members.is_empty() {
                continue;
            }
            let x0 = members.iter().map(|p| p.x).min().unwrap_or(0).saturating_sub(2);
            let x1 = members.iter().map(|p| p.x + p.w).max().unwrap_or(0) + 1;
            let y0 = members.iter().map(|p| p.y).min().unwrap_or(0).saturating_sub(2);
            let y1 = members.iter().map(|p| p.y + p.h).max().unwrap_or(0);
            let title = truncate(&g.title, 28);
            let w = (x1 + 1 - x0).max(width_of(&title) + 6);
            let h = y1 + 1 - y0;
            c.rect(x0, y0, w, h);
            if !title.is_empty() {
                group_titles.push((x0 + 2, y0, format!(" {title} ")));
            }
        }

        // 2) 간선 — 순서/층 축(u/v) 으로 계산하고 Canvas::hline/vline/draw 로 낸다.
        let mut labels: Vec<(usize, usize, String)> = Vec::new();
        for (si, seg) in plan.segments.iter().enumerate() {
            let e = &self.edges[seg.edge];
            let upper = &plan.placed[seg.top];
            let lower = &plan.placed[seg.bottom];
            let band_v = plan.band_start[upper.rank];
            let row = band_v + plan.seg_row[si].min(plan.band_rows[upper.rank].saturating_sub(1));
            let uu = center(upper.u(), upper.pu());
            let lu = center(lower.u(), lower.pu());
            let lu = if uu.abs_diff(lu) <= 1 { uu } else { lu };
            let line_v = '│';
            let line_h = '─';

            let from_v = if upper.dummy { upper.v() } else { upper.v() + upper.h };
            let to_v = if lower.dummy { lower.v() + lower.h - 1 } else { lower.v().saturating_sub(1) };
            if uu == lu {
                c.vline(uu, from_v, to_v, line_v);
            } else {
                if row > from_v {
                    c.vline(uu, from_v, row - 1, line_v);
                }
                c.hline(uu.min(lu) + 1, uu.max(lu) - 1, row, line_h);
                if to_v > row {
                    c.vline(lu, row + 1, to_v, line_v);
                }
                let (a, b) = if lu > uu { ('└', '┐') } else { ('┘', '┌') };
                c.draw(uu, row, a);
                c.draw(lu, row, b);
            }

            let is_first = plan.segments[..si].iter().all(|s| s.edge != seg.edge);
            let is_last = plan.segments[si + 1..].iter().all(|s| s.edge != seg.edge);
            if is_last && !lower.dummy {
                if let Some(ch) = arrow_char(e.head, true) {
                    c.draw(lu, to_v, ch);
                }
                if !e.head_tag.is_empty() {
                    c.text(lu + 2, to_v, &e.head_tag);
                }
            }
            if is_first && !upper.dummy {
                if let Some(ch) = arrow_char(e.tail, false) {
                    c.draw(uu, from_v, ch);
                }
                if !e.tail_tag.is_empty() {
                    c.text(uu + 2, from_v, &e.tail_tag);
                }
            }

            if !e.label.is_empty() && is_first {
                let lab = truncate(&e.label, 24);
                let lw = width_of(&lab);
                // 짧은/일직선 간선에서는 라벨이 `uu.max(lu)+2` 로, 카디널리티
                // 태그(head_tag/tail_tag)도 같은 `+2` 공식으로 앉아 정확히
                // 겹친다(리뷰 실측: `CUSTOMER ||--o{ ORDER : places` → "||aces").
                // 태그가 있으면 그 폭만큼 라벨을 더 밀어낸다.
                let tag_pad = [&e.tail_tag, &e.head_tag]
                    .into_iter()
                    .filter(|t| !t.is_empty())
                    .map(|t| width_of(t) + 1)
                    .max()
                    .unwrap_or(0);
                let lu0 = if uu.abs_diff(lu) < lw {
                    uu.max(lu) + 2 + tag_pad
                } else {
                    (uu.min(lu) + uu.max(lu)).div_ceil(2).saturating_sub(lw / 2)
                };
                labels.push((lu0, row, lab));
            }
        }

        // 2b) 되돌아가는 간선 — 층 축 통로를 타고 되돌아간다.
        for &(ei, gv) in &plan.gutters {
            let e = &self.edges[ei];
            let (src, tgt) = (&plan.placed[e.from], &plan.placed[e.to]);
            let su = center(src.u(), src.pu());
            let tv = tgt.v() + tgt.h / 2;
            let sv = plan.band_start[src.rank] + plan.fb_row.get(&ei).copied().unwrap_or(0);
            c.hline(su + 1, gv - 1, sv, '─');
            c.vline(gv, tv + 1, sv - 1, '│');
            c.hline(tgt.u() + tgt.pu() + 1, gv - 1, tv, '─');
            c.draw(su, sv, '└');
            c.draw(gv, sv, '┘');
            c.draw(gv, tv, '┐');
            if arrow_char(e.head, true).is_some() {
                c.draw(tgt.u() + tgt.pu(), tv, '\u{25c0}'); // ◀
            }
            if !e.label.is_empty() {
                labels.push((su + 2, sv, truncate(&e.label, 20)));
            }
        }

        for (x, y, t) in &group_titles {
            c.text(*x, *y, t);
        }
        // 리뷰가 실측한 버그: 카디널리티 태그(head_tag/tail_tag, 예: ER 의
        // `||`/`o{`)를 먼저 찍고 라벨을 나중에 쓰다 보니, 라벨 앞의 "빈 칸으로
        // 지우기"가 같은 칸에 이미 있던 태그 글자까지 지워버렸다(`CUSTOMER
        // ||--o{ ORDER : places` 에서 `||` 가 사라짐). 라벨은 이미 뭔가 그려진
        // 칸(태그·화살촉·박스 테두리)은 지우지도 덮어쓰지도 않는다 — 좁은 간격에
        // 라벨과 태그가 겹치면 라벨 쪽이 양보한다(태그가 실제 관계 정보다).
        //
        // 16.4 실측: gitea-github-schema.md 의 `access permissions` 라벨이
        // 세로 통로(│)에 가운데가 덮여 "ac│ess permissions" 로 보였다 —
        // 위 보호가 순수 연결선 글자(┌┐└┘─│├┤┬┴┼, `is_connector_char`)
        // 까지 지켜준 탓이다. 연결선은 라벨 자리보다 먼저 그려질 뿐인
        // 배선 통로일 뿐 카디널리티 태그·화살촉·박스 테두리처럼 지켜야 할
        // 의미 있는 정보가 아니다 — 라벨이 그 위를 덮어써 텍스트가 온전히
        // 보이게 한다(태그·화살촉·박스 테두리는 여전히 보호).
        for (u, v, lab) in &labels {
            let mut cx = 0usize;
            for ch in lab.chars() {
                let (rx, ry) = (*u + cx, *v);
                let existing = c.get(rx, ry);
                if existing == ' ' || super::super::canvas::is_connector_char(existing) {
                    c.text(rx, ry, &ch.to_string());
                }
                cx += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            }
        }

        // 2c) 자기 자신으로 가는 간선
        for &ei in &plan.selfloop {
            let e = &self.edges[ei];
            let p = &plan.placed[e.from];
            let mut text = String::from("\u{21ba}"); // ↺
            if !e.label.is_empty() {
                text.push(' ');
                text.push_str(&truncate(&e.label, 18));
            }
            c.text(p.x + p.w + 1, p.y + p.h / 2, &text);
        }

        // 3) 노드 상자 — 항상 실제 x/y/w/h, 항상 정방향 텍스트.
        for p in plan.placed.iter() {
            if p.dummy {
                continue;
            }
            let node = &self.nodes[p.node];
            c.clear_rect(p.x, p.y, p.w, p.h);
            draw_shape(&mut c, p.x, p.y, p.w, p.h, node.shape);
            for (i, text) in plan.lines[p.node].iter().enumerate() {
                let y = p.y + 1 + i;
                if text == DIVIDER {
                    c.hline(p.x, p.x + p.w - 1, y, '─');
                    c.set(p.x, y, '├');
                    c.set(p.x + p.w - 1, y, '┤');
                    continue;
                }
                let inner = p.w.saturating_sub(2);
                let before_divider = plan.lines[p.node][..i].iter().all(|l| l != DIVIDER);
                let head = node.header && i == 0;
                let centered = !node.left_align || head || before_divider;
                let off = if centered { inner.saturating_sub(width_of(text)) / 2 } else { 1 };
                c.text(p.x + 1 + off, y, text);
            }
        }
        c.into_lines()
    }
}

/// 16.1 층 내 행 래핑: 순서(barycenter)가 이미 정해진 각 층의 노드 목록을
/// 순서를 유지한 채, 순서축 크기 합(+`HGAP`)이 `width_budget` 을 넘지 않는
/// 연속 구간으로 그리디하게 나눠 새 층 목록을 만든다. 한 층이 이미 예산
/// 안에 들어오면 그대로 한 층으로 남는다(회귀 없음). 노드 하나가 이미
/// 예산을 넘더라도 그 노드 혼자 한 줄이 된다 — 더 쪼갤 수 없다(라벨
/// 축약/본문 접기가 앞선 사다리 단계에서 이미 그 크기를 줄이는 몫을 한다).
fn wrap_rows(ranks: &[Vec<usize>], order_size: impl Fn(usize) -> usize, width_budget: usize) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = Vec::new();
    for ids in ranks {
        let mut group: Vec<usize> = Vec::new();
        let mut w = 0usize;
        for &i in ids {
            let sz = order_size(i);
            let projected = if group.is_empty() { sz } else { w + HGAP + sz };
            if !group.is_empty() && projected > width_budget {
                out.push(std::mem::take(&mut group));
                w = 0;
            }
            w = if group.is_empty() { sz } else { w + HGAP + sz };
            group.push(i);
        }
        out.push(group);
    }
    out
}

fn longest_path(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(a, b) in edges {
        if a != b && a < n && b < n {
            adj[a].push(b);
        }
    }
    let mut state = vec![0u8; n];
    let mut back: Vec<(usize, usize)> = Vec::new();
    for s in 0..n {
        if state[s] != 0 {
            continue;
        }
        state[s] = 1;
        let mut stack = vec![(s, 0usize)];
        while let Some((v, i)) = stack.pop() {
            if i < adj[v].len() {
                stack.push((v, i + 1));
                let w = adj[v][i];
                match state[w] {
                    0 => {
                        state[w] = 1;
                        stack.push((w, 0));
                    }
                    1 => back.push((v, w)),
                    _ => {}
                }
            } else {
                state[v] = 2;
            }
        }
    }
    let mut indeg = vec![0usize; n];
    let mut fwd: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(a, b) in edges {
        if a == b || a >= n || b >= n || back.contains(&(a, b)) {
            continue;
        }
        fwd[a].push(b);
        indeg[b] += 1;
    }
    let mut rank = vec![0usize; n];
    let mut queue: Vec<usize> = (0..n).filter(|i| indeg[*i] == 0).collect();
    while let Some(v) = queue.pop() {
        for &w in &fwd[v] {
            rank[w] = rank[w].max(rank[v] + 1);
            indeg[w] -= 1;
            if indeg[w] == 0 {
                queue.push(w);
            }
        }
    }
    rank
}

fn node_rank(i: usize, n: usize, rank: &[usize], dummy_rank: &[usize]) -> usize {
    if i < n { rank[i] } else { dummy_rank[i - n] }
}

fn center(x: usize, w: usize) -> usize {
    x + w / 2
}

fn arrow_char(a: MArrow, down: bool) -> Option<char> {
    Some(match (a, down) {
        (MArrow::None, _) => return None,
        (MArrow::Open, true) => '\u{25bc}',         // ▼
        (MArrow::Open, false) => '\u{25b2}',        // ▲
        (MArrow::Hollow, true) => '\u{25bd}',       // ▽
        (MArrow::Hollow, false) => '\u{25b3}',      // △
        (MArrow::Diamond, _) => '\u{25c6}',         // ◆
        (MArrow::HollowDiamond, _) => '\u{25c7}',   // ◇
        // U+2715(✕)는 흔한 CJK 모노스페이스 폰트 커버리지 밖이라 폰트 폴백을
        // 유발한다(dg 엔진의 같은 수정 참고) — U+00D7(×)로.
        (MArrow::Cross, _) => '\u{d7}',              // ×
    })
}

fn draw_shape(c: &mut Canvas, x: usize, y: usize, w: usize, h: usize, shape: MShape) {
    match shape {
        MShape::Rect => c.rect(x, y, w, h),
        MShape::Round | MShape::Circle | MShape::Diamond => c.rect(x, y, w, h),
        MShape::Cylinder => {
            c.rect(x, y, w, h);
            if w >= 3 {
                c.hline(x + 1, x + w - 2, y, '\u{2550}');
                c.hline(x + 1, x + w - 2, y + h - 1, '\u{2550}');
            }
        }
        MShape::Subroutine => {
            c.rect(x, y, w, h);
            for yy in y + 1..y + h.saturating_sub(1) {
                c.set(x + 1, yy, '│');
                c.set(x + w.saturating_sub(2), yy, '│');
            }
        }
    }
    if shape == MShape::Diamond {
        c.set(x, y, '\u{25c7}'); // ◇
    }
}

/// Splits a label to fit `cap` columns (whitespace-first, then character
/// fallback) — used at each shrink-attempt `cap` in `Graph::render`.
fn wrap_label(s: &str, cap: usize) -> Vec<String> {
    let s = s.trim();
    if s.is_empty() {
        return vec![String::new()];
    }
    if width_of(s) <= cap {
        return vec![s.to_string()];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let ww = width_of(word);
        if cur.is_empty() {
            cur = word.to_string();
        } else if width_of(&cur) + 1 + ww <= cap {
            cur.push(' ');
            cur.push_str(word);
        } else {
            out.push(std::mem::take(&mut cur));
            cur = word.to_string();
        }
        while width_of(&cur) > cap {
            let mut head = String::new();
            let mut rest = String::new();
            let mut wsum = 0;
            for ch in cur.chars() {
                let cw = width_of(&ch.to_string());
                if wsum + cw <= cap && rest.is_empty() {
                    head.push(ch);
                    wsum += cw;
                } else {
                    rest.push(ch);
                }
            }
            out.push(head);
            cur = rest;
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Maps `sniff_kind`'s short dispatch tag ("class") to the actual mermaid
/// keyword ("classDiagram") for the `Fallback::{Empty,Overflow}` label shown
/// to the user (`code.rs` renders it as `"Mermaid (Overflow): {kind}"`).
/// `sniff_kind` itself is untouched — its short form is what `supports()`
/// and `render_mermaid`'s dispatch match on, and changing it would ripple
/// into `builtin.rs` too.
fn display_kind(short: &str) -> &'static str {
    match short {
        "er" => "erDiagram",
        "class" => "classDiagram",
        "state" => "stateDiagram",
        "flowchart" => "flowchart",
        _ => "generic",
    }
}

fn width_of(s: &str) -> usize {
    use unicode_width::UnicodeWidthChar;
    s.chars().map(|c| UnicodeWidthChar::width(c).unwrap_or(0)).sum()
}

/// Truncates to `max` display columns, appending `…` when it doesn't fit.
fn truncate(s: &str, max: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    if width_of(s) <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > max.saturating_sub(1) {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

// ---------------------------------------------------------------- engine

/// The `mdview` [`GraphEngine`] — supports every graph diagram kind (same
/// coverage as `builtin`), so nothing ever falls back off of it once
/// selected.
pub struct MdviewEngine;

impl GraphEngine for MdviewEngine {
    fn name(&self) -> &'static str {
        "mdview"
    }

    fn supports(&self, kind: &str) -> bool {
        matches!(kind, "flowchart" | "er" | "class" | "state" | "generic")
    }

    fn render(&self, src: &str, diagram: &Diagram, width: u16) -> Result<Vec<String>, Fallback> {
        // 16.2: `Graph::render`/`build_graph` 는 자기 실제 종류를 모른다(파서
        // 판정은 mod.rs 가 이미 한 일이라 여기서 다시 파싱하지 않는다) — 실제
        // 판정 종류는 `sniff_kind(src)` 로 얻어, 폴백에 하드코딩된 "flowchart"
        // 대신 실어 보낸다("Mermaid (Overflow): classDiagram" 처럼).
        // `sniff_kind`'s short form ("class") is what `supports()`/dispatch
        // use elsewhere and must stay unchanged; the fallback *label* should
        // read the actual mermaid keyword ("classDiagram") — a separate,
        // display-only mapping, not a change to sniff_kind's contract.
        let kind = display_kind(super::super::sniff_kind(src));
        let retag = |e: Fallback| -> Fallback {
            match e {
                Fallback::Empty { .. } => Fallback::Empty { kind: kind.to_string() },
                Fallback::Overflow { .. } => Fallback::Overflow { kind: kind.to_string() },
            }
        };
        let Some(graph) = build_graph(diagram) else {
            return Err(Fallback::Empty { kind: kind.to_string() });
        };
        if graph.nodes.is_empty() {
            return Err(Fallback::Empty { kind: kind.to_string() });
        }
        // 16.3: LR 재시도(14.5) 제거 — mdview 원본처럼 항상 TB 한 번만 시도.
        graph.render(width as usize).map_err(retag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::mermaid::parse;

    fn render(src: &str, width: u16) -> Vec<String> {
        let diagram = parse::parse(src).expect("parse should succeed");
        MdviewEngine.render(src, &diagram, width).expect("render should succeed at this width")
    }

    #[test]
    fn simple_tb_chain_stacks_top_to_bottom() {
        let lines = render("graph TB\n  A --> B\n  B --> C", 40);
        let ia = lines.iter().position(|l| l.contains('A')).unwrap();
        let ib = lines.iter().position(|l| l.contains('B')).unwrap();
        let ic = lines.iter().position(|l| l.contains('C')).unwrap();
        assert!(ia < ib && ib < ic, "TB chain should stack top to bottom: {lines:?}");
    }

    #[test]
    fn lr_declaration_is_ignored_renders_top_to_bottom() {
        // 16.3: `graph LR` 선언이라도 mdview 엔진은 방향을 무시하고 항상
        // TB(위→아래)로 낸다 — 참조 mdview 원본과 동일 동작(5.7 개정).
        let lines = render("graph LR\n  A --> B\n  B --> C", 60);
        let row = |ch: char| -> usize {
            lines.iter().position(|l| l.contains(ch)).unwrap_or(usize::MAX)
        };
        let (ra, rb, rc) = (row('A'), row('B'), row('C'));
        assert!(ra < rb && rb < rc, "LR declaration should still stack top to bottom: {lines:?}");
    }

    #[test]
    fn diverging_edges_get_separate_wiring_not_a_shared_bus() {
        // 5.20: 여러 간선이 같은 세로 통로에 뭉쳐 ┼ 로만 합쳐지면 출발·도착 추적이
        // 안 된다 — 적어도 서로 다른 열에서 각 간선이 시작해야 한다.
        let lines = render("graph TB\n  A --> B\n  A --> C\n  A --> D", 80);
        let row_of = |ch: char| lines.iter().position(|l| l.contains(ch)).unwrap();
        let (rb, rc, rd) = (row_of('B'), row_of('C'), row_of('D'));
        assert_eq!(rb, rc);
        assert_eq!(rc, rd);
        let col_of = |ch: char, row: usize| lines[row].find(ch).unwrap();
        let (cb, cc, cd) = (col_of('B', rb), col_of('C', rc), col_of('D', rd));
        assert!(cb != cc && cc != cd, "siblings must not collapse onto the same column: {lines:?}");
    }

    #[test]
    fn arrowhead_points_at_destination_not_source() {
        // 실제로 잡힌 버그(초판): edge_style() 이 (head, tail) 순서로 튜플을
        // 만들고 (tail, head) 로 destructure 해 화살표가 from(출발지)에
        // 찍혔다 — A→B 인데 "A" 쪽에 화살촉이 그려짐. 16.3 이후 항상 TB 라
        // LR 선언(`graph LR`)도 같은 TB 출력을 내야 한다 — 둘 다 확인한다.
        for src in ["graph TB\n  A --> B", "graph LR\n  A --> B"] {
            let lines = render(src, 40);
            let joined = lines.join("\n");
            let a_row = lines.iter().position(|l| l.contains('A')).unwrap();
            let b_row = lines.iter().position(|l| l.contains('B')).unwrap();
            assert!(a_row < b_row, "expected A above B regardless of declared direction: {lines:?}");
            // ▼(down arrow) 는 A와 B 사이 어딘가에, ▲(up, 즉 출발지 쪽 화살표)는
            // 전혀 없어야 한다 — Arrow 스타일은 tail=None 이다.
            assert!(joined.contains('\u{25bc}'), "expected ▼ pointing into B: {joined}");
            assert!(!joined.contains('\u{25b2}'), "unexpected ▲ at the source: {joined}");
        }
    }

    #[test]
    fn feedback_edge_does_not_panic_and_produces_output() {
        let lines = render("graph TB\n  A --> B\n  B --> C\n  C --> A", 80);
        assert!(!lines.is_empty());
        assert!(lines.iter().any(|l| l.contains('A')));
    }

    #[test]
    fn lr_synthetic_long_chain_retries_as_tb_when_lr_overflows() {
        // 16.3 이후: 더 이상 "재시도"가 아니다 — 애초에 항상 TB 이므로 층이
        // 세로로 쌓여 폭 40 에도 그대로 들어간다(LR 이었다면 층 수만큼 가로폭이
        // 필요해 넘쳤을 6노드 체인). 회귀 확인용으로 이름/시나리오는 유지.
        let src = "graph LR\n  NodeAlpha --> NodeBravo --> NodeCharlie --> NodeDelta --> NodeEcho --> NodeFoxtrot";
        let lines = render(src, 40);
        let joined = lines.join("\n");
        assert!(!joined.is_empty());
        assert!(joined.contains("NodeAlpha"), "expected a real diagram, not source fallback: {joined}");
        assert!(joined.contains('┌'), "expected a rendered box, not a source-text fallback: {joined}");
    }

    /// design.md 원본 마크다운에서 "### Boundary Map" 바로 다음
    /// ` ```mermaid ` 펜스 본문을 뽑아낸다 — 이 테스트는 전역 엔진
    /// 레지스트리(`super::super::select`)를 거치지 않고 `MdviewEngine` 을
    /// 직접 호출한다: `cargo test` 는 테스트를 여러 스레드로 병렬 실행하고
    /// 엔진 선택은 프로세스 전역 `OnceLock<RwLock<..>>` 이라, 다른 테스트가
    /// (예: engine/mod.rs 의 builtin 선택 테스트) 동시에 돌면 이 테스트의
    /// `select("mdview")` 이후 `render()` 호출 사이에 전역 선택이 바뀔 수
    /// 있다(실측: full-suite 로 돌리면 가끔 builtin 출력이 섞여 나왔다) —
    /// 엔진을 직접 호출하면 이 경합 자체가 없다.
    fn extract_boundary_map_fence(design_md: &str) -> String {
        let idx = design_md.find("### Boundary Map").expect("design.md must have a Boundary Map heading");
        let after = &design_md[idx..];
        let fence_start = after.find("```mermaid\n").expect("Boundary Map heading must be followed by a mermaid fence") + "```mermaid\n".len();
        let fence_end = after[fence_start..].find("```").expect("mermaid fence must be closed");
        after[fence_start..fence_start + fence_end].to_string()
    }

    #[test]
    fn real_boundary_map_matches_reference_mdview_tb_output_at_100_and_120() {
        // 16.3 — 기계적 검증: LR 전치·14.5 재시도를 걷어내고 mdview 원본
        // 배선 그대로 동작하는지, design.md 의 실제 Boundary Map 소스를 120·
        // 100 열에서 이 엔진으로 렌더한 문자열이 참조 바이너리
        // `~/dev/tools/mdview/target/release/mdview -P -w N design.md` 출력의
        // 다이어그램 부분과 diff 0 인지로 확인한다. 전치본은 박스 오른쪽
        // 변에 `┐┌┬─▶` 겹침·통로 곳곳의 `┼` 가 있었고(리뷰 실측), 원본은
        // `├`/`└──┐`/`▼` 로 깨끗하다 — 그 차이를 그대로 실패로 잡아낸다.
        // 참조 바이너리가 로컬에 없을 수 있으므로(이번 확인 당시 실측: 삭제
        // 되어 없었음), 이번에 실제로 그 바이너리를 실행해 만들어 둔
        // 스냅샷(tests/fixtures/mdview-tb/boundary-map-w{100,120}.txt)과
        // 비교한다 — 바이너리가 있으면 그 자리에서 다시 만들어 우선 사용한다.
        let design_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mermaid-samples/spec-viewer/design.md");
        let design_md = std::fs::read_to_string(design_path).expect("this repo's own design.md must exist for this check");
        let mermaid_src = extract_boundary_map_fence(&design_md);
        let diagram = parse::parse(&mermaid_src).expect("Boundary Map source must parse as a flowchart");

        // Reference binary: `MDVIEW_BIN` env, else `mdview` on PATH. Absent → stored snapshot below.
        let ref_bin = std::env::var_os("MDVIEW_BIN")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("PATH").and_then(|paths| {
                    std::env::split_paths(&paths).map(|d| d.join("mdview")).find(|c| c.is_file())
                })
            })
            .unwrap_or_else(|| std::path::PathBuf::from("mdview"));

        for width in [100u16, 120] {
            let ours = MdviewEngine
                .render(&mermaid_src, &diagram, width)
                .unwrap_or_else(|e| panic!("width {width} should render a real diagram, not fall back to source: {e:?}"))
                .join("\n")
                .trim_end()
                .to_string();

            let reference: String = if ref_bin.exists() {
                let out = std::process::Command::new(&ref_bin)
                    .args(["-P", "--no-color", "-w", &width.to_string(), design_path])
                    .output()
                    .unwrap_or_else(|e| panic!("failed to run reference mdview binary: {e}"));
                assert!(out.status.success(), "reference mdview binary exited non-zero");
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                let ref_lines: Vec<&str> = text.lines().collect();
                let start = ref_lines.iter().position(|l| l.contains("◈ mermaid")).expect("reference output must contain a mermaid header") + 1;
                let end = ref_lines[start..].iter().position(|l| l.trim().is_empty()).map(|i| start + i).unwrap_or(ref_lines.len());
                ref_lines[start..end].iter().map(|l| l.strip_prefix("    ").unwrap_or(l)).collect::<Vec<_>>().join("\n")
            } else {
                std::fs::read_to_string(format!(
                    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mdview-tb/boundary-map-w{}.txt"),
                    width
                ))
                .expect("reference binary missing — its stored snapshot must exist instead")
                .trim_end()
                .to_string()
            };

            assert_eq!(ours, reference, "width {width}: our TB output must diff 0 against the reference mdview binary/snapshot");
        }
    }

    #[test]
    fn overflow_fallback_kind_reflects_actual_diagram_type_not_hardcoded_flowchart() {
        // 16.2: engine/mdview.rs 는 폴백 kind 로 항상 "flowchart" 를
        // 하드코딩했었다 — classDiagram 이 넘쳐도 "Mermaid (Overflow):
        // flowchart" 로 잘못 표시됐다. 실제 판정 종류(mod.rs::sniff_kind)를
        // 사람이 읽는 mermaid 키워드로 바꿔("class"→"classDiagram") 실어야
        // 한다.
        let src = "classDiagram\n  class VeryLongClassNameThatWillNotFitInThreeColumns {\n    +String someAttribute\n  }";
        let diagram = parse::parse(src).unwrap();
        let err = MdviewEngine.render(src, &diagram, 3).expect_err("width 3 must overflow");
        let kind = match err {
            Fallback::Overflow { kind } | Fallback::Empty { kind } => kind,
        };
        assert_eq!(kind, "classDiagram", "fallback kind must name the real diagram type, not a hardcoded default");
    }

    #[test]
    fn large_class_diagram_renders_at_100_and_120_without_overflow() {
        // 16.1 — 실측(gitea-github-schema.md, 15클래스·20관계, 16.1 조사 중
        // class.rs 파서 버그 두 건을 고쳐 15/20 으로 확정됨): 파서 수정
        // 이후에도 클래스 박스 본문(속성/메서드)이 항상 최소 22열로 묶여
        // 있어(sizes()의 `cap.max(22)`) 폭 100/120 에서 여전히 소스 폴백
        // (Mermaid (Overflow): classDiagram)으로 떨어졌다 — 이 테스트는 그
        // 재현을 고정한다. '본문 접기'(속성/메서드를 버리고 이름만 남긴
        // 상자로 재시도)와 '층 내 행 래핑'(한 층이 그래도 넘치면 그 층의
        // 노드를 여러 시각적 줄로 나눠 쌓기)을 추가해야 통과한다.
        let schema_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mermaid-samples/spec-viewer/gitea-github-schema.md");
        let schema_md = std::fs::read_to_string(schema_path).expect("this repo's own gitea-github-schema.md fixture must exist");
        let fence_start = schema_md.find("```mermaid\n").expect("fixture must have a mermaid fence") + "```mermaid\n".len();
        let fence_end = schema_md[fence_start..].find("```").expect("mermaid fence must be closed");
        let mermaid_src = &schema_md[fence_start..fence_start + fence_end];
        let diagram = parse::parse(mermaid_src).expect("fixture source must parse as a classDiagram");

        for width in [100u16, 120] {
            let lines = MdviewEngine
                .render(mermaid_src, &diagram, width)
                .unwrap_or_else(|e| panic!("width {width} should render a real diagram, not fall back to source: {e:?}"));
            let joined = lines.join("\n");
            assert!(joined.contains("Organization") && joined.contains("Team"), "expected real class node labels at width {width}: {joined}");
        }
    }

    #[test]
    fn edge_label_text_is_never_split_by_a_crossing_connector_line() {
        // 16.4 — 실측: gitea-github-schema.md 를 렌더하면 `Team "N" o--
        // "M" Repository : access permissions` 간선의 라벨이 세로 통로
        // 문자(│)에 가운데가 덮여 "ac│ess permissions" 로 보인다. 라벨을
        // 통로보다 먼저 그리든 나중에 그리든, 라벨 텍스트 칸은 순수
        // 연결선(┌┐└┘─│├┤┬┴┼) 이 대신 차지해선 안 된다 — 카디널리티
        // 태그·화살촉·박스 테두리 보호(기존 리뷰 수정)는 그대로 유지한다.
        let schema_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mermaid-samples/spec-viewer/gitea-github-schema.md");
        let schema_md = std::fs::read_to_string(schema_path).expect("this repo's own gitea-github-schema.md fixture must exist");
        let fence_start = schema_md.find("```mermaid\n").expect("fixture must have a mermaid fence") + "```mermaid\n".len();
        let fence_end = schema_md[fence_start..].find("```").expect("mermaid fence must be closed");
        let mermaid_src = &schema_md[fence_start..fence_start + fence_end];
        let diagram = parse::parse(mermaid_src).expect("fixture source must parse as a classDiagram");

        for width in [100u16, 120] {
            let lines = MdviewEngine
                .render(mermaid_src, &diagram, width)
                .unwrap_or_else(|e| panic!("width {width} should render a real diagram, not fall back to source: {e:?}"));
            let joined = lines.join("\n");
            assert!(
                joined.contains("access permissions"),
                "width {width}: edge label must render intact, not split by a connector line: {joined}"
            );
        }
    }

    #[test]
    fn er_entity_renders_with_divider_and_fields() {
        let src = "erDiagram\n  CUSTOMER {\n    string id\n    string name\n  }";
        let lines = render(src, 60);
        let joined = lines.join("\n");
        assert!(joined.contains("CUSTOMER"));
        assert!(joined.contains("id"));
        assert!(joined.contains("name"));
    }

    #[test]
    fn cardinality_tags_survive_alongside_a_label() {
        // 실제로 잡힌 버그: 태그(||, o{)를 즉시 그린 뒤 라벨을 나중에 쓰면서
        // 라벨 앞의 빈칸 블랭킹이 같은 칸의 태그 글자를 지워버렸다 — 라벨이
        // 있는 카디널리티 간선에서만 재현된다(태그 단독으로는 안 걸림).
        let src = "erDiagram\n  CUSTOMER ||--o{ ORDER : places";
        let joined = render(src, 60).join("\n");
        assert!(joined.contains("||"), "tail cardinality || missing when a label is present: {joined}");
        assert!(joined.contains("o{"), "head cardinality o{{ missing when a label is present: {joined}");
        assert!(joined.contains("places"), "label itself missing: {joined}");
    }

    #[test]
    fn state_start_and_end_markers_present() {
        let src = "stateDiagram-v2\n  [*] --> A\n  A --> [*]";
        let lines = render(src, 60);
        let joined = lines.join("\n");
        assert!(joined.contains('\u{25cf}') || joined.contains('\u{25c9}'), "expected a start/end marker: {joined}");
    }
}
