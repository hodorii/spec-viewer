use super::canvas::Canvas;
use super::parse::{Diagram, Dir, Edge, EdgeStyle, Node, Shape, Subgraph};
use super::Fallback;
use crate::markdown::wrap::line_width;
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

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

fn dedupe_abbreviations(ids: &[String], budget: usize) -> HashMap<String, String> {
    let mut unique: Vec<String> = Vec::new();
    for id in ids {
        if !unique.contains(id) {
            unique.push(id.clone());
        }
    }
    let mut map: HashMap<String, String> = unique.iter().map(|id| (id.clone(), abbreviate(id, budget))).collect();

    let max_rounds = unique.len().saturating_add(4);
    for _ in 0..max_rounds {
        let mut groups: HashMap<String, Vec<String>> = HashMap::new();
        for id in &unique {
            groups.entry(map[id].clone()).or_default().push(id.clone());
        }
        let colliding: Vec<Vec<String>> = groups.into_values().filter(|v| v.len() > 1).collect();
        if colliding.is_empty() {
            break;
        }
        for members in &colliding {
            let max_needed = members.iter().map(|m| line_width(m)).max().unwrap_or(budget);
            let mut cur_b = members.iter().map(|m| line_width(&map[m])).max().unwrap_or(budget);
            let mut resolved = false;
            while cur_b < max_needed {
                cur_b += 1;
                let abbrevs: Vec<String> = members.iter().map(|m| abbreviate(m, cur_b)).collect();
                let mut sorted = abbrevs.clone();
                sorted.sort();
                sorted.dedup();
                if sorted.len() == abbrevs.len() {
                    for (m, a) in members.iter().zip(abbrevs) {
                        map.insert(m.clone(), a);
                    }
                    resolved = true;
                    break;
                }
            }
            if !resolved {
                for m in members {
                    map.insert(m.clone(), m.clone());
                }
            }
        }
    }
    map
}

fn id_disp(id: &str, budget: Option<usize>, id_map: Option<&HashMap<String, String>>) -> String {
    match id_map {
        Some(m) => m.get(id).cloned().unwrap_or_else(|| trunc(id, budget)),
        None => trunc(id, budget),
    }
}

fn pad_to_width(s: &str, w: usize) -> String {
    let pad = w.saturating_sub(line_width(s));
    format!("{}{}", s, " ".repeat(pad))
}

fn pad_row(s: &str, w: usize) -> String {
    let pad = w.saturating_sub(line_width(s));
    format!("│{}{}│", s, " ".repeat(pad))
}

fn render_box(lines: &[String]) -> Vec<String> {
    let w = lines.iter().map(|l| line_width(l)).max().unwrap_or(0);
    let mut out = vec![format!("┌{}┐", "─".repeat(w))];
    for l in lines {
        out.push(pad_row(l, w));
    }
    out.push(format!("└{}┘", "─".repeat(w)));
    out
}

fn node_box(node: &Node, budget: Option<usize>, id_map: Option<&HashMap<String, String>>) -> Vec<String> {
    match &node.shape {
        Shape::Box(label) => {
            let content = if label.is_empty() || label == &node.id {
                id_disp(&node.id, budget, id_map)
            } else {
                trunc(label, budget)
            };
            render_box(&[content])
        }
        Shape::State(name) => render_box(&[trunc(name, budget)]),
        Shape::Start => vec!["●".to_string()],
        Shape::End => vec!["◉".to_string()],
        Shape::Entity { name, fields } => {
            let header = trunc(name, budget);
            let field_lines: Vec<String> = fields.iter().map(|f| trunc(f, budget)).collect();
            let w = std::iter::once(&header)
                .chain(field_lines.iter())
                .map(|l| line_width(l))
                .max()
                .unwrap_or(0);
            let mut out = vec![format!("┌{}┐", "─".repeat(w)), pad_row(&header, w), format!("├{}┤", "─".repeat(w))];
            for f in &field_lines {
                out.push(pad_row(f, w));
            }
            out.push(format!("└{}┘", "─".repeat(w)));
            out
        }
        Shape::Class { name, attrs, methods } => {
            let header = trunc(name, budget);
            let attr_lines: Vec<String> = attrs.iter().map(|a| trunc(a, budget)).collect();
            let method_lines: Vec<String> = methods.iter().map(|m| trunc(m, budget)).collect();
            let w = std::iter::once(&header)
                .chain(attr_lines.iter())
                .chain(method_lines.iter())
                .map(|l| line_width(l))
                .max()
                .unwrap_or(0);
            let mut out = vec![format!("┌{}┐", "─".repeat(w)), pad_row(&header, w), format!("├{}┤", "─".repeat(w))];
            for a in &attr_lines {
                out.push(pad_row(a, w));
            }
            out.push(format!("├{}┤", "─".repeat(w)));
            for m in &method_lines {
                out.push(pad_row(m, w));
            }
            out.push(format!("└{}┘", "─".repeat(w)));
            out
        }
    }
}

fn node_label_width(n: &Node) -> usize {
    match &n.shape {
        Shape::Box(l) => line_width(if l.is_empty() { &n.id } else { l }),
        Shape::State(s) => line_width(s),
        Shape::Start | Shape::End => 1,
        Shape::Entity { name, fields } => {
            std::iter::once(name).chain(fields.iter()).map(|s| line_width(s)).max().unwrap_or(0)
        }
        Shape::Class { name, attrs, methods } => std::iter::once(name)
            .chain(attrs.iter())
            .chain(methods.iter())
            .map(|s| line_width(s))
            .max()
            .unwrap_or(0),
    }
}

fn glyph_ends(style: &EdgeStyle, lr: bool) -> (String, String) {
    match style {
        EdgeStyle::Arrow | EdgeStyle::Transition => {
            (String::new(), if lr { "►".to_string() } else { "▼".to_string() })
        }
        EdgeStyle::Line => (String::new(), String::new()),
        EdgeStyle::Cardinality(l, r) => (l.clone(), r.clone()),
        EdgeStyle::Inherit => (String::new(), "▷".to_string()),
        EdgeStyle::Compose => ("◆".to_string(), String::new()),
        EdgeStyle::Aggregate => ("◇".to_string(), String::new()),
    }
}

/// Text overlaid directly on top of a real connector line: only what
/// mermaid itself specifies for this edge — its style's leading glyph (e.g.
/// `◆`/`◇` for compose/aggregate, or a cardinality glyph, taken from the
/// diagram's own syntax such as `A ||--o{ B`) and/or its label, if any.
/// Never the source node's id: an edge with no explicit mermaid label
/// (`-->|label|` / `-- label -->`) specifies no text at all, so a
/// label-less, glyph-less edge renders zero overlay text — only the line
/// and its arrowhead (placed separately, at the strip/band's far end, by
/// `place_arrowhead`).
fn edge_overlay_text(style: &EdgeStyle, label: &Option<String>, lr: bool) -> String {
    let (ns, _nt) = glyph_ends(style, lr);
    let label = label.as_deref().filter(|l| !l.is_empty());
    match (ns.is_empty(), label) {
        (true, None) => String::new(),
        (true, Some(l)) => l.to_string(),
        (false, None) => ns,
        (false, Some(l)) => format!("{ns} {l}"),
    }
}

/// Hard-overwrites the trailing glyph (arrowhead / cardinality-right marker)
/// at the far end of a line, wide-glyph aware.
fn place_arrowhead(canvas: &mut Canvas, w: usize, row: usize, glyph: &str) {
    if glyph.is_empty() {
        return;
    }
    let gw = line_width(glyph);
    let x = w.saturating_sub(gw);
    canvas.text(x, row, glyph);
}

type ResolvedEdge<'a> = (usize, usize, &'a Edge);
type XBucket<'a> = (usize, Vec<ResolvedEdge<'a>>);

fn compute_layers(n: usize, resolved: &[(usize, usize, &Edge)]) -> Vec<usize> {
    let mut layer = vec![0usize; n];
    for _ in 0..n.max(1) {
        let mut changed = false;
        for &(f, t, _) in resolved {
            if f == t {
                continue;
            }
            if layer[t] < layer[f] + 1 {
                layer[t] = layer[f] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    layer
}

fn group_by_layer(layer: &[usize]) -> Vec<Vec<usize>> {
    let max_layer = layer.iter().copied().max().unwrap_or(0);
    let mut layers_idx: Vec<Vec<usize>> = vec![Vec::new(); max_layer + 1];
    for (i, &l) in layer.iter().enumerate() {
        layers_idx[l].push(i);
    }
    layers_idx
}

fn render_lr(
    nodes: &[Node],
    layers_idx: &[Vec<usize>],
    resolved: &[(usize, usize, &Edge)],
    layer: &[usize],
    budget: Option<usize>,
    id_map: Option<&HashMap<String, String>>,
) -> Vec<String> {
    let boxes: Vec<Vec<String>> = nodes.iter().map(|n| node_box(n, budget, id_map)).collect();
    let box_w: Vec<usize> = boxes.iter().map(|b| b.iter().map(|l| line_width(l)).max().unwrap_or(0)).collect();

    let mut col_lines: Vec<Vec<String>> = Vec::new();
    let mut col_width: Vec<usize> = Vec::new();
    let mut node_row: HashMap<usize, usize> = HashMap::new();

    for col in layers_idx {
        let w = col.iter().map(|&i| box_w[i]).max().unwrap_or(0);
        let mut lines: Vec<String> = Vec::new();
        for &i in col {
            let start = lines.len();
            for l in &boxes[i] {
                lines.push(pad_to_width(l, w));
            }
            node_row.insert(i, start + boxes[i].len() / 2);
            lines.push(" ".repeat(w));
        }
        if !lines.is_empty() {
            lines.pop();
        }
        col_width.push(w);
        col_lines.push(lines);
    }

    let num_bounds = layers_idx.len().saturating_sub(1);
    let mut per_bound: Vec<Vec<(usize, usize, &Edge)>> = vec![Vec::new(); num_bounds];
    for &(f, t, e) in resolved {
        if f == t {
            continue;
        }
        if layer[t] == 0 {
            continue;
        }
        let b = layer[t] - 1;
        if b >= num_bounds {
            continue;
        }
        per_bound[b].push((f, t, e));
    }

    // Determine each edge's landing row within its boundary. Collision
    // avoidance (bumping the landing row down when another edge already
    // claimed it) mirrors the pre-existing behavior: multiple sources
    // converging on the same target row each still get their own row, so
    // they stay individually traceable instead of merging into one
    // indistinguishable line. `canvas_h` grows to fit as before.
    let mut canvas_h = col_lines.iter().map(|c| c.len()).max().unwrap_or(0);
    struct LrDraw<'a> {
        row_f: usize,
        land: usize,
        e: &'a Edge,
    }
    let mut bound_draws: Vec<Vec<LrDraw>> = Vec::with_capacity(num_bounds);
    for edges_here in &per_bound {
        let mut es: Vec<(usize, usize, &Edge)> = edges_here.clone();
        es.sort_by_key(|&(f, t, _)| (*node_row.get(&t).unwrap_or(&0), *node_row.get(&f).unwrap_or(&0), f));
        let mut used = std::collections::HashSet::new();
        let mut draws = Vec::new();
        for (f, t, e) in es {
            let mut land = *node_row.get(&t).unwrap_or(&0);
            while used.contains(&land) {
                land += 1;
            }
            used.insert(land);
            if land + 1 > canvas_h {
                canvas_h = land + 1;
            }
            let row_f = *node_row.get(&f).unwrap_or(&0);
            draws.push(LrDraw { row_f, land, e });
        }
        bound_draws.push(draws);
    }

    // A shared bend column near the left edge of each boundary's strip: the
    // line leaves the source at row_f, jogs down/up to the target's landing
    // row at this column, then runs the rest of the strip width to the
    // target. Keeping the jog near the left leaves most of the strip's
    // width for the label+arrowhead, which sit on this final segment.
    const LR_BEND_OFFSET: usize = 1;

    let strip_w: Vec<usize> = bound_draws
        .iter()
        .map(|draws| {
            draws
                .iter()
                .map(|d| {
                    let (_ns, nt) = glyph_ends(&d.e.style, true);
                    let overlay = edge_overlay_text(&d.e.style, &d.e.label, true);
                    let margin = if d.row_f == d.land { 1 } else { LR_BEND_OFFSET };
                    line_width(&overlay) + line_width(&nt) + margin + 1
                })
                .max()
                .unwrap_or(4)
                .max(4)
        })
        .collect();

    let mut strip_lines: Vec<Vec<String>> = Vec::with_capacity(num_bounds);
    for (b, draws) in bound_draws.into_iter().enumerate() {
        let w = strip_w[b];
        let mut canvas = Canvas::new(w, canvas_h);
        // Pass 1: draw every edge's line skeleton first, so a later edge's
        // line segment never stomps an earlier edge's already-placed text
        // (e.g. another edge sharing this edge's source row for its own
        // leading segment).
        for d in &draws {
            if d.row_f == d.land {
                canvas.hline(0, w.saturating_sub(1), d.row_f, '─');
            } else {
                let bend_x = LR_BEND_OFFSET.min(w.saturating_sub(1)).max(1);
                canvas.hline(0, bend_x, d.row_f, '─');
                canvas.vline(bend_x, d.row_f, d.land, '│');
                canvas.hline(bend_x, w.saturating_sub(1), d.land, '─');
            }
        }
        // Pass 2: overlay each edge's label/arrowhead on top of the lines.
        for d in &draws {
            let (_ns, nt) = glyph_ends(&d.e.style, true);
            let overlay = edge_overlay_text(&d.e.style, &d.e.label, true);
            if d.row_f == d.land {
                canvas.text(1.min(w.saturating_sub(1)), d.row_f, &overlay);
                place_arrowhead(&mut canvas, w, d.row_f, &nt);
            } else {
                let bend_x = LR_BEND_OFFSET.min(w.saturating_sub(1)).max(1);
                let text_x = (bend_x + 1).min(w.saturating_sub(1));
                canvas.text(text_x, d.land, &overlay);
                place_arrowhead(&mut canvas, w, d.land, &nt);
            }
        }
        strip_lines.push(canvas.into_lines());
    }

    let mut out = Vec::with_capacity(canvas_h);
    for r in 0..canvas_h {
        let mut line = String::new();
        for c in 0..col_lines.len() {
            let cell = col_lines[c].get(r).map(|s| s.as_str()).unwrap_or("");
            line.push_str(&pad_to_width(cell, col_width[c]));
            if c < strip_lines.len() {
                let strip_cell = strip_lines[c].get(r).map(|s| s.as_str()).unwrap_or("");
                line.push_str(&pad_to_width(strip_cell, strip_w[c]));
            }
        }
        out.push(line);
    }
    out
}

fn render_tb(
    nodes: &[Node],
    layers_idx: &[Vec<usize>],
    resolved: &[(usize, usize, &Edge)],
    layer: &[usize],
    budget: Option<usize>,
    id_map: Option<&HashMap<String, String>>,
) -> Vec<String> {
    let boxes: Vec<Vec<String>> = nodes.iter().map(|n| node_box(n, budget, id_map)).collect();
    let box_w: Vec<usize> = boxes.iter().map(|b| b.iter().map(|l| line_width(l)).max().unwrap_or(0)).collect();
    let box_h: Vec<usize> = boxes.iter().map(|b| b.len()).collect();

    let mut row_lines: Vec<Vec<String>> = Vec::new();
    let mut node_col: HashMap<usize, usize> = HashMap::new();

    for row in layers_idx {
        let h = row.iter().map(|&i| box_h[i]).max().unwrap_or(0);
        let mut lines = vec![String::new(); h];
        let mut x = 0usize;
        for (idx, &i) in row.iter().enumerate() {
            if idx > 0 {
                for l in lines.iter_mut() {
                    l.push_str("  ");
                }
                x += 2;
            }
            let w = box_w[i];
            node_col.insert(i, x + w / 2);
            for (li, line) in lines.iter_mut().enumerate() {
                let content = boxes[i].get(li).map(|s| s.as_str()).unwrap_or("");
                line.push_str(&pad_to_width(content, w));
            }
            x += w;
        }
        row_lines.push(lines);
    }

    let canvas_w = row_lines
        .iter()
        .map(|r| r.iter().map(|l| line_width(l)).max().unwrap_or(0))
        .max()
        .unwrap_or(0);

    let num_bounds = layers_idx.len().saturating_sub(1);
    let mut per_bound: Vec<Vec<(usize, usize, &Edge)>> = vec![Vec::new(); num_bounds];
    for &(f, t, e) in resolved {
        if f == t {
            continue;
        }
        if layer[t] == 0 {
            continue;
        }
        let b = layer[t] - 1;
        if b >= num_bounds {
            continue;
        }
        per_bound[b].push((f, t, e));
    }

    // Group each boundary's edges by target column so that multiple sources
    // converging on the same target column each get their own "lane"
    // (a distinct row within the band for their overlay text), just like
    // the pre-existing target-row collision avoidance did in render_lr —
    // otherwise their labels would overwrite each other and traceability
    // would be lost.
    let mut band_lines: Vec<Vec<String>> = Vec::with_capacity(num_bounds);
    for edges_here in &per_bound {
        // A boundary with no edges routed through it (e.g. layers inflated
        // by compute_layers's pre-existing cycle-handling simplification,
        // out of this task's scope, can leave many boundaries between real
        // layers empty) contributes no band rows at all — matching the old
        // implementation, which dropped every all-blank row via `retain`.
        if edges_here.is_empty() {
            band_lines.push(Vec::new());
            continue;
        }
        let mut buckets: Vec<XBucket> = Vec::new();
        for &(f, t, e) in edges_here {
            let x = *node_col.get(&t).unwrap_or(&0);
            match buckets.iter_mut().find(|(bx, _)| *bx == x) {
                Some(entry) => entry.1.push((f, t, e)),
                None => buckets.push((x, vec![(f, t, e)])),
            }
        }
        for (_, v) in buckets.iter_mut() {
            v.sort_by_key(|&(f, _, _)| f);
        }
        let max_lanes = buckets.iter().map(|(_, v)| v.len()).max().unwrap_or(0);
        // Row 0 carries the line only (touching the row-group above); rows
        // 1..=max_lanes each carry one lane's overlay text "partway down"
        // the line; the final row carries the arrowhead (touching the
        // row-group below).
        let band_height = max_lanes.max(1) + 2;
        let bottom = band_height - 1;

        // Precompute each edge's overlay/arrow text and required width
        // (the overlay can run well past the node's own column, e.g. a
        // long edge label — the band must be at least that wide, the same
        // way render_lr's strip_w grows to fit its overlay text).
        struct TbDraw {
            col_f: usize,
            col_t: usize,
            lane: usize,
            overlay: String,
            nt: String,
        }
        let mut draws: Vec<TbDraw> = Vec::new();
        let mut w = canvas_w.max(1);
        for (_x, v) in &buckets {
            for (lane, &(f, t, e)) in v.iter().enumerate() {
                let col_f = node_col.get(&f).copied().unwrap_or(0);
                let col_t = node_col.get(&t).copied().unwrap_or(0);
                let (_ns, nt) = glyph_ends(&e.style, false);
                let overlay = edge_overlay_text(&e.style, &e.label, false);
                let needed = col_t + line_width(&overlay).max(line_width(&nt)) + 1;
                w = w.max(needed);
                draws.push(TbDraw { col_f, col_t, lane, overlay, nt });
            }
        }

        let mut canvas = Canvas::new(w, band_height);
        // Pass 1: draw every edge's line skeleton first, so a later edge's
        // line segment never stomps an earlier edge's already-placed text.
        for d in &draws {
            let col_f = d.col_f.min(w.saturating_sub(1));
            let col_t = d.col_t.min(w.saturating_sub(1));
            if col_f == col_t {
                canvas.vline(col_t, 0, bottom, '│');
            } else {
                // A shared bend row per boundary: the line runs straight
                // down from the source column, jogs horizontally to the
                // target column, then straight down into the target —
                // draw()'s bitmask merging turns the jog corners (and any
                // other edge's line crossing through them) into real
                // junction characters instead of one overwriting the other.
                let bend_y = (band_height / 2).min(bottom.saturating_sub(1)).max(1);
                canvas.vline(col_f, 0, bend_y, '│');
                canvas.hline(col_f, col_t, bend_y, '─');
                canvas.vline(col_t, bend_y, bottom, '│');
            }
        }
        // Pass 2: overlay each edge's label/arrowhead on top of the lines.
        for d in &draws {
            let col_t = d.col_t.min(w.saturating_sub(1));
            let text_row = (1 + d.lane).min(bottom.saturating_sub(1).max(1));
            canvas.text(col_t, text_row, &d.overlay);
            if !d.nt.is_empty() {
                let gw = line_width(&d.nt);
                let x = col_t.min(w.saturating_sub(gw));
                canvas.text(x, bottom, &d.nt);
            }
        }
        band_lines.push(canvas.into_lines());
    }

    let mut out: Vec<String> = Vec::new();
    for (ri, row) in row_lines.iter().enumerate() {
        for l in row {
            out.push(pad_to_width(l, canvas_w));
        }
        if ri < band_lines.len() {
            for l in &band_lines[ri] {
                out.push(pad_to_width(l, canvas_w));
            }
        }
    }
    out
}

fn dashed_box(inner: &[String], title: &str) -> Vec<String> {
    let content_w = inner.iter().map(|l| line_width(l)).max().unwrap_or(0);
    let title_disp = if title.is_empty() { String::new() } else { format!("┄┄ {} ", title) };
    let title_w = line_width(&title_disp);
    let w = content_w.max(title_w);
    let dash_fill = w.saturating_sub(title_w);
    let top = format!("┌{}{}┐", title_disp, "┄".repeat(dash_fill));
    let mut out = vec![top];
    for l in inner {
        let pad = w.saturating_sub(line_width(l));
        out.push(format!("┆{}{}┆", l, " ".repeat(pad)));
    }
    out.push(format!("└{}┘", "┄".repeat(w)));
    out
}

fn render_subgraph(
    nodes: &[Node],
    group: &Subgraph,
    budget: Option<usize>,
    id_map: Option<&HashMap<String, String>>,
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut member_boxes: Vec<Vec<String>> = Vec::new();
    for id in &group.members {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(nd) = nodes.iter().find(|n| &n.id == id) {
            member_boxes.push(node_box(nd, budget, id_map));
        }
    }
    if member_boxes.is_empty() {
        return dashed_box(&[], &group.title);
    }
    let h = member_boxes.iter().map(|b| b.len()).max().unwrap_or(0);
    let mut inner: Vec<String> = vec![String::new(); h];
    for (i, b) in member_boxes.iter().enumerate() {
        let w = b.iter().map(|l| line_width(l)).max().unwrap_or(0);
        for (row, line) in inner.iter_mut().enumerate() {
            if i > 0 {
                line.push_str("  ");
            }
            let content = b.get(row).map(|s| s.as_str()).unwrap_or("");
            line.push_str(&pad_to_width(content, w));
        }
    }
    dashed_box(&inner, &group.title)
}

pub fn layout(diagram: &Diagram, width: u16, kind: &str) -> Result<Vec<String>, Fallback> {
    let (dir, nodes, edges, groups) = match diagram {
        Diagram::Graph { dir, nodes, edges, groups } => (dir, nodes, edges, groups),
        _ => return Err(Fallback::Empty { kind: kind.to_string() }),
    };

    if nodes.is_empty() {
        return Err(Fallback::Empty { kind: kind.to_string() });
    }

    let n = nodes.len();
    let resolved: Vec<(usize, usize, &Edge)> = edges
        .iter()
        .filter_map(|e| {
            let f = nodes.iter().position(|nd| nd.id == e.from)?;
            let t = nodes.iter().position(|nd| nd.id == e.to)?;
            Some((f, t, e))
        })
        .collect();

    let layer = compute_layers(n, &resolved);
    let layers_idx = group_by_layer(&layer);

    let natural_max_label = nodes.iter().map(node_label_width).max().unwrap_or(1).max(1);
    let all_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();

    let mut budget: Option<usize> = None;
    let mut result: Option<Vec<String>> = None;

    let max_attempts = natural_max_label.saturating_add(1);
    for _ in 0..=max_attempts {
        let id_map = budget.map(|b| dedupe_abbreviations(&all_ids, b));
        let mut lines = match dir {
            Dir::LR => render_lr(nodes, &layers_idx, &resolved, &layer, budget, id_map.as_ref()),
            Dir::TB => render_tb(nodes, &layers_idx, &resolved, &layer, budget, id_map.as_ref()),
        };
        if !groups.is_empty() {
            lines.push(String::new());
            for g in groups {
                lines.extend(render_subgraph(nodes, g, budget, id_map.as_ref()));
            }
        }
        let fits = lines.iter().all(|l| line_width(l) <= width as usize);
        if fits {
            result = Some(lines);
            break;
        }
        let next = match budget {
            None => natural_max_label.saturating_sub(1).max(MIN_BUDGET),
            Some(b) if b > MIN_BUDGET => b.saturating_sub(1),
            _ => break,
        };
        if Some(next) == budget {
            break;
        }
        budget = Some(next);
    }

    result.ok_or_else(|| Fallback::Overflow { kind: kind.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::parse;

    fn joined(lines: &[String]) -> String {
        lines.join("\n")
    }

    #[test]
    fn test_boundary_map_lr_width80_and_120() {
        let src = "graph LR\n    FS[kiro directory] --> Loader\n    Loader --> SpecModel\n    Loader --> Render\n    Render --> Rendered\n    SpecModel --> TreePanel\n    Rendered --> DocPanel\n    Rendered --> Search\n    Rendered --> Toc\n    Watcher --> Reducer\n    Keys --> Reducer\n    Reducer --> Loader\n    Reducer --> AppState\n    AppState --> TreePanel\n    AppState --> DocPanel\n    AppState --> StatusBar";
        let diagram = parse::parse(src).unwrap();
        // At width 80 (and even 120, now that connectors are annotated with
        // their source id per 5.7) this 6-layer diagram may need label
        // abbreviation (per 5.16); only direction/arrows/fit are guaranteed
        // there. At width 150 it fits comfortably unabbreviated, so every
        // id/label is checked verbatim there.
        for width in [80u16, 120u16] {
            let lines = layout(&diagram, width, "flowchart").unwrap();
            let text = joined(&lines);
            assert!(!lines.is_empty(), "expected non-empty render at width {width}");
            assert!(text.contains('►'), "expected LR arrow head at width {width}");
            for l in &lines {
                assert!(line_width(l) <= width as usize, "line too wide at width {width}: {l}");
            }
        }

        let lines150 = layout(&diagram, 150, "flowchart").unwrap();
        let text150 = joined(&lines150);
        for id in [
            "kiro directory", "Loader", "SpecModel", "Render", "Rendered", "TreePanel", "DocPanel", "Search", "Toc",
            "Watcher", "Reducer", "AppState", "StatusBar",
        ] {
            assert!(text150.contains(id), "missing {id} at width 150 (should fit unabbreviated)");
        }

        // Structural regression check for the previously-REJECTED review
        // finding: connectors used to be positioned by the target's local
        // row only, so `Loader --> Render` rendered visually indistinguishable
        // from a fabricated `AppState --> Render` (AppState shares Loader's
        // column and happens to sit on Render's own landing row). Task 9.6
        // removed all source-name overlay text (mermaid specifies none for
        // this label-less edge), so this can no longer be checked via a
        // glued source name — verify the real wiring geometrically instead:
        // an unbroken vertical connector column must run from Loader's own
        // row down to Render's own row (proving the line's true origin),
        // landing in a junction that feeds Render's arrowhead. AppState sits
        // physically beside that landing point but is not itself the
        // column's source.
        let loader_row = lines150
            .iter()
            .position(|l| l.contains("│Loader│"))
            .unwrap_or_else(|| panic!("expected Loader's own box, got:\n{text150}"));
        let render_row = lines150
            .iter()
            .position(|l| l.contains("│Render│"))
            .unwrap_or_else(|| panic!("expected Render's own box, got:\n{text150}"));
        assert_ne!(loader_row, render_row, "this regression check requires a genuine bend (Loader and Render on different rows)");

        let loader_end = char_pos(&lines150[loader_row], "│Loader│").unwrap() + "│Loader│".chars().count();
        let bend_col = find_junction_col(&lines150[loader_row], loader_end).unwrap_or_else(|| {
            panic!("expected a bend junction on Loader's own row after its box, got: {}", lines150[loader_row])
        });
        assert!(
            unbroken_vertical(&lines150, loader_row, render_row, bend_col),
            "expected an unbroken vertical connector from Loader's row (line {loader_row}) down to Render's row \
             (line {render_row}) at column {bend_col}, got:\n{text150}"
        );
        assert!(
            arrow_reachable_from_junction(&lines150[render_row], bend_col),
            "expected the vertical connector's landing junction (column {bend_col}) to lead directly \
             (skipping only line-drawing dashes) to an arrowhead into Render, got: {}",
            lines150[render_row]
        );

        // `AppState --> TreePanel` and `SpecModel --> TreePanel` converge on
        // the same target from different, non-adjacent layers. Both edges
        // must land on TreePanel, and never be concatenated into one
        // ambiguous shared row (the original bug) — each keeps its own row
        // within the boundary (target-row collision avoidance): one lands
        // directly on TreePanel's own labeled row, the other is bumped onto
        // TreePanel's neighboring border row, so their arrowheads must land
        // on two distinct rows, each genuinely touching TreePanel's box.
        let treepanel_label_row = lines150
            .iter()
            .position(|l| l.contains("│TreePanel│"))
            .unwrap_or_else(|| panic!("expected TreePanel's own box, got:\n{text150}"));
        let treepanel_border_row = treepanel_label_row + 1;
        assert!(
            lines150[treepanel_label_row].contains("►│TreePanel│"),
            "expected an arrowhead landing directly on TreePanel's own labeled row, got: {}",
            lines150[treepanel_label_row]
        );
        assert!(
            lines150.get(treepanel_border_row).is_some_and(|l| l.contains('►') && l.contains('└')),
            "expected the second converging edge's arrowhead bumped onto TreePanel's neighboring border row \
             (distinct from its labeled row), got: {:?}",
            lines150.get(treepanel_border_row)
        );
    }

    /// The char index (not byte index) of the first occurrence of `needle`
    /// in `line`, wide-glyph-agnostic since it counts `char`s not display
    /// columns — fine here since every glyph these tests search for/around
    /// is single-width.
    fn char_pos(line: &str, needle: &str) -> Option<usize> {
        let chars: Vec<char> = line.chars().collect();
        let needle: Vec<char> = needle.chars().collect();
        if needle.is_empty() || needle.len() > chars.len() {
            return None;
        }
        (0..=chars.len() - needle.len()).find(|&i| chars[i..i + needle.len()] == needle[..])
    }

    /// The char index of the first bend/merge junction (`┼`/`┬`) on `line`
    /// at or after `from`, i.e. the point where a vertical connector departs
    /// a horizontal run.
    fn find_junction_col(line: &str, from: usize) -> Option<usize> {
        line.chars().enumerate().skip(from).find(|&(_, c)| c == '┼' || c == '┬').map(|(i, _)| i)
    }

    /// True if there is a real, unbroken vertical connector at char-column
    /// `col` spanning every row strictly between `row_a` and `row_b` — i.e.
    /// a genuine physical link between the two rows (each intervening row's
    /// character there carries a vertical edge), not a coincidence of two
    /// separate, unconnected segments that merely happen to render on
    /// nearby rows.
    fn unbroken_vertical(lines: &[String], row_a: usize, row_b: usize, col: usize) -> bool {
        let (a, b) = if row_a <= row_b { (row_a, row_b) } else { (row_b, row_a) };
        if b <= a + 1 {
            return true; // adjacent rows: nothing between them to check
        }
        ((a + 1)..b).all(|r| matches!(lines[r].chars().nth(col), Some('│' | '┼' | '├' | '┤')))
    }

    /// True if, starting just past the junction at char-column `col` on
    /// `line` and skipping only line-drawing dash fill (`─`), the very next
    /// non-dash character is an LR arrowhead (`►`) — i.e. this junction's
    /// line genuinely terminates in that arrowhead, with no other edge's
    /// content sitting between them.
    fn arrow_reachable_from_junction(line: &str, col: usize) -> bool {
        let chars: Vec<char> = line.chars().collect();
        let mut i = col + 1; // skip the junction character itself
        while i < chars.len() && chars[i] == '─' {
            i += 1;
        }
        chars.get(i) == Some(&'►')
    }

    #[test]
    fn test_boundary_map_render_rendered_distinguishable_at_width_80() {
        // Regression for REJECTED review finding #1: at width 80 (the same
        // Boundary Map fixture from design.md as above), the sibling ids
        // `Render` and `Rendered` (connected by `Render --> Rendered`) used
        // to both abbreviate to the identical text `Ren…`, making the boxes
        // and their connector source-annotations indistinguishable.
        let src = "graph LR\n    FS[kiro directory] --> Loader\n    Loader --> SpecModel\n    Loader --> Render\n    Render --> Rendered\n    SpecModel --> TreePanel\n    Rendered --> DocPanel\n    Rendered --> Search\n    Rendered --> Toc\n    Watcher --> Reducer\n    Keys --> Reducer\n    Reducer --> Loader\n    Reducer --> AppState\n    AppState --> TreePanel\n    AppState --> DocPanel\n    AppState --> StatusBar";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80, "flowchart").unwrap();
        let text = joined(&lines);
        for l in &lines {
            assert!(line_width(l) <= 80, "line too wide at width 80: {l}");
        }

        let render_box_line = lines
            .iter()
            .find(|l| l.contains("│Render│"))
            .unwrap_or_else(|| panic!("expected Render's own box to render distinctly, got:\n{text}"));
        let rendered_box_line = lines
            .iter()
            .find(|l| l.contains("Rende"))
            .unwrap_or_else(|| panic!("expected Rendered's own box to render (possibly abbreviated), got:\n{text}"));

        assert_ne!(
            render_box_line, rendered_box_line,
            "Render and Rendered must not render onto an indistinguishable identical line: {render_box_line}"
        );
        assert!(
            !text.contains("Ren…"),
            "abbreviating both Render and Rendered down to the identical `Ren…` reintroduces the ambiguity bug, got:\n{text}"
        );

        // Also verify directly at the abbreviation-logic level: the ids
        // `Render` and `Rendered` must map to distinct abbreviated strings
        // at the budget actually chosen for this width-80 render.
        if let Diagram::Graph { nodes, .. } = &diagram {
            let ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
            for budget in 3..=8usize {
                let map = dedupe_abbreviations(&ids, budget);
                assert_ne!(
                    map.get("Render"),
                    map.get("Rendered"),
                    "Render and Rendered must be pairwise distinguishable at budget {budget}"
                );
            }
        }
    }

    #[test]
    fn test_lr_edge_label_rendered() {
        let src = "graph LR\n  A[Start] -->|go| B[End]";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80, "flowchart").unwrap();
        let text = joined(&lines);
        assert!(text.contains("go"));
        assert!(text.contains('►'));
    }

    #[test]
    fn test_labelless_edge_shows_no_source_name_on_connector() {
        // Task 9.6 repro: `A --> B` carries no mermaid label (no `|label|`,
        // no `-- text -->`), so mermaid itself specifies zero text for this
        // connector — only the line and its arrowhead. The overlay must not
        // glue the source id `A` onto the line.
        let src = "graph LR\n A --> B";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80, "flowchart").unwrap();
        let text = joined(&lines);
        assert!(text.contains('►'), "expected an arrowhead, got:\n{text}");
        // "A" must appear only inside A's own box (`│A│`), never elsewhere
        // on the connector strip between the two boxes (which sits on the
        // very same line as A's box, just further right — so strip out only
        // the box substring itself before checking, not the whole line).
        for l in &lines {
            let stripped = l.replace("│A│", "");
            assert!(!stripped.contains('A'), "label-less connector must not show source id 'A', got line: {l:?}\nfull:\n{text}");
        }
    }

    #[test]
    fn test_labeled_edge_shows_only_label_no_source_name() {
        let src = "graph LR\n A -->|checks| B";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80, "flowchart").unwrap();
        let text = joined(&lines);
        assert!(text.contains("checks"), "expected the label 'checks', got:\n{text}");
        assert!(text.contains('►'), "expected an arrowhead, got:\n{text}");
        for l in &lines {
            let stripped = l.replace("│A│", "");
            assert!(!stripped.contains('A'), "labeled connector must not show source id 'A' next to its label, got line: {l:?}\nfull:\n{text}");
        }
    }

    #[test]
    fn test_er_fixture_entities_attrs_cardinality() {
        let src = "erDiagram\n  CUSTOMER {\n    string name\n    string id\n  }\n  ORDER {\n    string sku\n  }\n  CUSTOMER ||--o{ ORDER : places";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80, "er").unwrap();
        let text = joined(&lines);
        assert!(text.contains("CUSTOMER"));
        assert!(text.contains("ORDER"));
        assert!(text.contains("string name"));
        assert!(text.contains("string id"));
        assert!(text.contains("string sku"));
        assert!(text.contains("||"));
        assert!(text.contains("o{"));
        assert!(text.contains("places"));
    }

    #[test]
    fn test_class_fixture_three_compartments_and_inherit() {
        let src = "classDiagram\n  class Animal {\n    +String name\n    +eat()\n  }\n  class Dog {\n    +String breed\n    +bark()\n  }\n  Animal <|-- Dog";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80, "class").unwrap();
        let text = joined(&lines);
        assert!(text.contains("Animal"));
        assert!(text.contains("Dog"));
        assert!(text.contains("+String name"));
        assert!(text.contains("+eat()"));
        let divider_count = lines.iter().filter(|l| l.contains('├')).count();
        assert!(divider_count >= 2, "expected at least 2 compartment dividers total, got {divider_count}");
        // Task 9.5: TB connectors overlay the source id and the arrowhead on
        // separate band rows (the label sits "partway down" the vertical
        // line, per the design), not glued into one string like before, so
        // check the same underlying property directly: Animal's overlay
        // text sits immediately above the row bearing the inheritance
        // glyph, joined by a real vertical line.
        assert!(text.contains('▷'), "expected inheritance glyph ▷, got: {text}");
        // Task 9.6: the inherit edge carries no mermaid label, so the
        // connector band now shows zero overlay text (not even the source
        // id, unlike task 9.5's "Animal" band-row text) — verify
        // traceability geometrically instead: the row bearing the
        // inheritance glyph sits below Animal's own box, connected to it by
        // an unbroken vertical line, with no stray source-id text glued
        // anywhere on that link.
        let animal_line = lines
            .iter()
            .position(|l| l.contains("Animal"))
            .unwrap_or_else(|| panic!("expected Animal's box, got:\n{text}"));
        let animal_box_end = (animal_line..lines.len())
            .find(|&i| lines[i].trim_start().starts_with('└'))
            .unwrap_or_else(|| panic!("expected the end (closing border) of Animal's box, got:\n{text}"));
        let arrow_row = lines
            .iter()
            .position(|l| l.contains('▷'))
            .unwrap_or_else(|| panic!("expected a row bearing the inheritance glyph, got:\n{text}"));
        assert!(
            arrow_row > animal_box_end,
            "expected the inheritance glyph below Animal's box, got:\n{text}"
        );
        for r in (animal_box_end + 1)..arrow_row {
            assert!(
                lines[r].contains('│'),
                "expected an unbroken vertical connector from Animal's box down to the inheritance glyph, missing at row {r}:\n{text}"
            );
        }
        for r in (animal_box_end + 1)..=arrow_row {
            assert!(
                !lines[r].contains("Animal"),
                "no stray source-id text expected on the label-less connector, got row {r}: {}",
                lines[r]
            );
        }
        assert!(
            lines.iter().any(|l| l.contains('│')),
            "expected a real vertical connecting line between Animal and Dog, got:\n{text}"
        );
    }

    #[test]
    fn test_compose_and_aggregate_glyphs() {
        let src = "classDiagram\n  Car *-- Engine\n  Car o-- Wheel";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80, "class").unwrap();
        let text = joined(&lines);
        // Task 9.6: the compose/aggregate glyph is mermaid-specified (from
        // the diagram's own `*--`/`o--` syntax), so it still renders — but
        // it must never be glued to the source id (this edge carries no
        // mermaid label, so no text should ever attach to "Car").
        assert!(text.contains('◆'), "expected compose glyph, got: {text}");
        assert!(text.contains('◇'), "expected aggregate glyph, got: {text}");
        assert!(!text.contains("Car◆") && !text.contains("Car ◆"), "compose glyph must not be glued to source id 'Car', got: {text}");
        assert!(!text.contains("Car◇") && !text.contains("Car ◇"), "aggregate glyph must not be glued to source id 'Car', got: {text}");
        assert!(
            lines.iter().any(|l| l.contains('┼') || l.contains('┬') || l.contains('├') || l.contains('┤')),
            "expected Car's two outgoing edges to merge into a real junction, got:\n{text}"
        );
    }

    #[test]
    fn test_state_fixture_start_end_and_label() {
        let src = "stateDiagram-v2\n  [*] --> A\n  A --> B : label\n  B --> [*]";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80, "state").unwrap();
        let text = joined(&lines);
        assert!(text.contains('●'), "expected start marker");
        assert!(text.contains('◉'), "expected end marker");
        assert!(text.contains("label"));
        assert!(text.contains('A'));
        assert!(text.contains('B'));
    }

    #[test]
    fn test_state_star_as_both_source_and_target_no_crash() {
        let src = "stateDiagram-v2\n  [*] --> A\n  A --> [*]\n  [*] --> B";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80, "state");
        assert!(lines.is_ok(), "must not crash on duplicate [*] ids");
        let text = joined(&lines.unwrap());
        assert!(text.contains('●'));
        assert!(text.contains('◉'));
    }

    #[test]
    fn test_generic_gantt_fixture_renders_as_box_graph() {
        let src = "gantt\n    title Adoption Timeline\n    section Planning\n    Kickoff :done, 2024-01-01, 3d\n    Research :active, 2024-01-04, 5d\n    Kickoff --> Research";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80, "generic").unwrap();
        let text = joined(&lines);
        assert!(text.contains("Kickoff"));
        assert!(text.contains("Research"));
        assert!(text.contains("done, 2024-01-01, 3d"));
        assert!(text.contains('▼') || text.contains('►'));
    }

    #[test]
    fn test_width_40_overflow_falls_back() {
        // A single long label can always be abbreviated down to fit, so the
        // unfixable-overflow case is structural: too many short, already-minimal
        // sibling boxes crammed into one layer/row for any abbreviation to help.
        let src = "graph TB\n  A --> B1\n  A --> B2\n  A --> B3\n  A --> B4\n  A --> B5\n  A --> B6\n  A --> B7\n  A --> B8\n  A --> B9\n  A --> B10";
        let diagram = parse::parse(src).unwrap();
        let res = layout(&diagram, 40, "flowchart");
        assert!(matches!(res, Err(Fallback::Overflow { .. })), "expected overflow fallback, got {:?}", res);
    }

    #[test]
    fn test_width_40_abbreviation_succeeds() {
        let src = "graph TB\n  A[This descriptive node label is definitely longer than forty display columns wide] --> B[Short]";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 40, "flowchart").unwrap();
        let text = joined(&lines);
        assert!(text.contains('…'), "expected abbreviation ellipsis, got: {text}");
        for l in &lines {
            assert!(line_width(l) <= 40, "line exceeds width 40: {l}");
        }
    }

    #[test]
    fn test_subgraph_dashed_border_and_title() {
        let src = "graph LR\n  subgraph Group1\n    A\n    B\n  end\n  A --> B";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80, "flowchart").unwrap();
        let text = joined(&lines);
        assert!(text.contains("Group1"));
        assert!(text.contains('┄'));
        assert!(text.contains('┆'));
    }

    #[test]
    fn test_korean_label_width_no_panic_and_correct_width() {
        let src = "graph TB\n  A[한글 레이블 노드] --> B[다른 노드]";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80, "flowchart").unwrap();
        let text = joined(&lines);
        assert!(text.contains("한글 레이블 노드"));
        for l in &lines {
            assert!(line_width(l) <= 80);
        }
    }

    #[test]
    fn test_empty_nodes_defensive_no_panic() {
        let diagram = Diagram::Graph { dir: Dir::TB, nodes: Vec::new(), edges: Vec::new(), groups: Vec::new() };
        let res = layout(&diagram, 80, "flowchart");
        assert!(matches!(res, Err(Fallback::Empty { .. })));
    }

    #[test]
    fn test_line_style_edge_no_arrowhead() {
        let src = "graph TB\n  A --- B";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80, "flowchart").unwrap();
        for l in &lines {
            assert!(!l.contains('▼'), "Line style must not render an arrowhead: {l}");
        }
    }

    #[test]
    fn test_lr_crossing_edges_traceable_to_correct_source() {
        // Two nodes per layer with a genuine crossing: A's real target (Y)
        // sits in the row that B's box would occupy under the old
        // target-row-only positioning, and vice versa for B/X. Standalone
        // node lines fix the column order deterministically:
        // column0=[A,B], column1=[X,Y].
        //
        // Task 9.6 removed all source-name overlay text, so a naive
        // misattribution regression (target-row-only positioning) can no
        // longer be caught via a glued source name — these edges carry
        // distinct mermaid labels instead, which is exactly the kind of
        // evidence still available: each label must land on its true
        // target's own row and nowhere else.
        let src = "graph LR\n  A\n  B\n  X\n  Y\n  A -->|toY| Y\n  B -->|toX| X";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 120, "flowchart").unwrap();
        let text = joined(&lines);

        let y_row = lines
            .iter()
            .position(|l| l.contains("│Y│"))
            .unwrap_or_else(|| panic!("expected a line rendering Y's box, got:\n{text}"));
        let x_row = lines
            .iter()
            .position(|l| l.contains("│X│"))
            .unwrap_or_else(|| panic!("expected a line rendering X's box, got:\n{text}"));
        assert_ne!(y_row, x_row, "X and Y must render on distinct rows for this crossing check to be meaningful");

        assert!(lines[y_row].contains("toY"), "A-->Y's label must land on Y's own row, got: {}", lines[y_row]);
        assert!(!lines[y_row].contains("toX"), "B-->X's label must not land on Y's row, got: {}", lines[y_row]);
        assert!(lines[x_row].contains("toX"), "B-->X's label must land on X's own row, got: {}", lines[x_row]);
        assert!(!lines[x_row].contains("toY"), "A-->Y's label must not land on X's row, got: {}", lines[x_row]);

        // The two edges' bends share a bend column per boundary (task 9.5),
        // so a genuine crossing must produce a real junction character
        // rather than one edge's line silently overwriting the other's.
        assert!(
            lines.iter().any(|l| "┼├┤┬┴".chars().any(|j| l.contains(j))),
            "expected the crossing edges to merge into a real junction, got:\n{text}"
        );
    }

    #[test]
    fn test_self_loop_edge_dropped_without_fabricated_connector() {
        let src = "graph LR\n  A --> A\n  A --> B";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80, "flowchart");
        assert!(lines.is_ok(), "self-loop edge must not panic or overflow the outer bound: {lines:?}");
        let lines = lines.unwrap();
        let text = joined(&lines);
        assert!(text.contains('A'));
        assert!(text.contains('B'));
        // Task 9.6 removed all source-name overlay text, so "the real
        // A-->B edge still renders" can no longer be checked via a glued
        // source name — verify it geometrically instead: A and B's boxes
        // must share a single row (a clean, unbent, direct connector) with
        // an arrowhead on it, proving the self-loop did not displace or
        // corrupt the real edge's routing.
        let a_row = lines.iter().position(|l| l.contains("│A│")).unwrap_or_else(|| panic!("expected A's box, got:\n{text}"));
        let b_row = lines.iter().position(|l| l.contains("│B│")).unwrap_or_else(|| panic!("expected B's box, got:\n{text}"));
        assert_eq!(
            a_row, b_row,
            "the real A-->B edge must render as a single-row direct connector, got:\n{text}"
        );
        assert!(lines[a_row].contains('►'), "expected an arrowhead on A/B's shared row, got: {}", lines[a_row]);
        // The self-loop must not fabricate a second, unrelated connector or
        // duplicate arrowhead: exactly one LR arrow head total.
        assert_eq!(text.matches('►').count(), 1, "self-loop must not fabricate an extra connector, got:\n{text}");
    }

    #[test]
    fn test_self_loop_does_not_corrupt_other_node_layers() {
        let src = "graph TB\n  A --> A\n  A --> B\n  B --> C";
        let diagram = parse::parse(src).unwrap();
        let lines = layout(&diagram, 80, "flowchart").unwrap();
        let text = joined(&lines);
        assert!(text.contains('A') && text.contains('B') && text.contains('C'));
        // B and C must still lay out in 3 distinct rows (A, then B, then C):
        // if the self-loop's non-convergence had corrupted layer[A], B/C's
        // relative layering (and thus this row ordering) would break.
        let a_row = lines.iter().position(|l| l.contains('A')).unwrap();
        let b_row = lines.iter().position(|l| l.contains('B')).unwrap();
        let c_row = lines.iter().position(|l| l.contains('C')).unwrap();
        assert!(a_row < b_row && b_row < c_row, "expected A above B above C, got:\n{text}");
    }

    #[test]
    fn test_self_loop_on_mid_chain_node_does_not_inflate_layers() {
        // Regression for REJECTED review finding #2: the previous self-loop
        // tests only ever put the self-loop on a root (layer-0) node, where
        // `compute_layers`'s unrelated `if layer[t] == 0 { continue; }`
        // render-time filter happens to mask any layer inflation the
        // self-loop guard would otherwise cause. Proven by mutation: with
        // the `if f == t { continue; }` guard in `compute_layers` removed,
        // B's self-loop (`B --> B`) makes `layer[t] < layer[f] + 1` always
        // true for f == t == B, so `changed` never settles within the
        // bounded `n.max(1)` outer iterations, and B (and everything
        // downstream of B) inflates every iteration instead of settling at
        // its real distance from A.
        let src = "graph LR\n  A --> B\n  B --> B\n  B --> C\n  C --> D";
        let diagram = parse::parse(src).unwrap();
        let (nodes, edges) = match &diagram {
            Diagram::Graph { nodes, edges, .. } => (nodes, edges),
            _ => panic!("expected a Graph diagram"),
        };
        let resolved: Vec<(usize, usize, &Edge)> = edges
            .iter()
            .filter_map(|e| {
                let f = nodes.iter().position(|nd| nd.id == e.from)?;
                let t = nodes.iter().position(|nd| nd.id == e.to)?;
                Some((f, t, e))
            })
            .collect();
        let layer = compute_layers(nodes.len(), &resolved);
        let idx_of = |id: &str| nodes.iter().position(|nd| nd.id == id).unwrap();

        assert_eq!(layer[idx_of("A")], 0, "A must be at layer 0");
        assert_eq!(layer[idx_of("B")], 1, "B's self-loop must not inflate B's own layer");
        assert_eq!(layer[idx_of("C")], 2, "C must sit exactly one layer past B, not inflated by B's self-loop");
        assert_eq!(layer[idx_of("D")], 3, "D must sit exactly one layer past C, not inflated by B's self-loop");

        let lines = layout(&diagram, 80, "flowchart").unwrap();
        for l in &lines {
            assert!(line_width(l) <= 80, "line too wide at width 80: {l}");
        }
    }
}
