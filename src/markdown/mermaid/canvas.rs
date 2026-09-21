//! Character-grid canvas for compositing mermaid diagram connector lines.
//! Box-drawing characters merge via directional bitmasks, so two lines
//! crossing automatically become a junction (┼, ├, ...) instead of one
//! overwriting the other. Ported (algorithm only, restyled to plain `char`
//! cells since this module's output is unstyled) from mdview's
//! src/render/canvas.rs — see THIRD_PARTY.md.

use unicode_width::UnicodeWidthChar;

/// Marker for the trailing cell of a wide (CJK) glyph.
const CONT: char = '\u{0}';

const UP: u8 = 1;
const RIGHT: u8 = 2;
const DOWN: u8 = 4;
const LEFT: u8 = 8;

/// Box-drawing char <-> directional bitmask.
const BOX_CHARS: [(char, u8); 11] = [
    ('─', LEFT | RIGHT),
    ('│', UP | DOWN),
    ('┌', RIGHT | DOWN),
    ('┐', LEFT | DOWN),
    ('└', UP | RIGHT),
    ('┘', UP | LEFT),
    ('├', UP | RIGHT | DOWN),
    ('┤', UP | DOWN | LEFT),
    ('┬', LEFT | RIGHT | DOWN),
    ('┴', LEFT | RIGHT | UP),
    ('┼', UP | RIGHT | DOWN | LEFT),
];

fn box_mask(ch: char) -> Option<u8> {
    BOX_CHARS.iter().find(|(c, _)| *c == ch).map(|(_, m)| *m)
}

/// Whether `ch` is one of the plain connector/junction glyphs this canvas
/// itself draws (`─│┌┐└┘├┤┬┴┼`) — never a cardinality tag, arrowhead, or
/// node-box text. Callers (16.4: edge-label placement) use this to allow
/// overwriting a *wire* a label crosses without also allowing a label to
/// eat a cardinality tag/arrowhead/box border, which must stay protected.
pub(super) fn is_connector_char(ch: char) -> bool {
    box_mask(ch).is_some()
}

fn mask_char(mask: u8) -> char {
    BOX_CHARS.iter().find(|(_, m)| *m == mask).map(|(c, _)| *c).unwrap_or('┼')
}

pub(super) struct Canvas {
    w: usize,
    h: usize,
    cells: Vec<char>,
}

impl Canvas {
    pub(super) fn new(w: usize, h: usize) -> Self {
        Canvas { w, h, cells: vec![' '; w * h] }
    }

    fn idx(&self, x: usize, y: usize) -> Option<usize> {
        if x < self.w && y < self.h {
            Some(y * self.w + x)
        } else {
            None
        }
    }

    pub(super) fn get(&self, x: usize, y: usize) -> char {
        self.idx(x, y).map(|i| self.cells[i]).unwrap_or(' ')
    }

    /// Overwrite a cell outright (no junction merging). Clears the partner
    /// cell of a wide glyph that either sits at this cell or is overwritten
    /// by it.
    pub(super) fn set(&mut self, x: usize, y: usize, ch: char) {
        let Some(i) = self.idx(x, y) else { return };
        // If we're overwriting the trailing half of a wide glyph, clear its
        // leading half too.
        if self.cells[i] == CONT && x > 0 {
            let j = i - 1;
            self.cells[j] = ' ';
        }
        // If we're overwriting the leading half of a wide glyph, clear its
        // trailing half too.
        if x + 1 < self.w && self.cells[i + 1] == CONT && UnicodeWidthChar::width(self.cells[i]).unwrap_or(1) == 2 {
            self.cells[i + 1] = ' ';
        }
        self.cells[i] = ch;
    }

    /// Box-drawing chars merge with whatever's already there via the
    /// directional bitmask; anything else just overwrites.
    pub(super) fn draw(&mut self, x: usize, y: usize, ch: char) {
        let old = self.get(x, y);
        match (box_mask(old), box_mask(ch)) {
            (Some(a), Some(b)) if a != b => self.set(x, y, mask_char(a | b)),
            _ => self.set(x, y, ch),
        }
    }

    #[allow(dead_code)]
    pub(super) fn draw_soft(&mut self, x: usize, y: usize, ch: char) {
        if self.get(x, y) == ' ' {
            self.set(x, y, ch);
        }
    }

    /// Writes `s` starting at `(x, y)`, wide-char aware. Returns the width used.
    pub(super) fn text(&mut self, x: usize, y: usize, s: &str) -> usize {
        let mut cx = x;
        for ch in s.chars() {
            if ch == '\n' {
                break;
            }
            let w = UnicodeWidthChar::width(ch).unwrap_or(0);
            if w == 0 {
                continue;
            }
            if cx >= self.w {
                break;
            }
            self.set(cx, y, ch);
            if w == 2 {
                if cx + 1 < self.w {
                    if let Some(i) = self.idx(cx + 1, y) {
                        self.cells[i] = CONT;
                    }
                } else {
                    // Only half fits in the last column; drop it.
                    self.set(cx, y, ' ');
                }
            }
            cx += w;
        }
        cx.saturating_sub(x)
    }

    /// `x0..=x1` horizontal line.
    pub(super) fn hline(&mut self, x0: usize, x1: usize, y: usize, ch: char) {
        let (a, b) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        for x in a..=b {
            self.draw(x, y, ch);
        }
    }

    /// `y0..=y1` vertical line.
    pub(super) fn vline(&mut self, x: usize, y0: usize, y1: usize, ch: char) {
        let (a, b) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        for y in a..=b {
            self.draw(x, y, ch);
        }
    }

    /// Square-cornered rectangle border (task 14.2 — mdview engine subgraph/
    /// node borders; ported unstyled from mdview's `Canvas::rect`, dropping
    /// the `round` corner variant since this crate's shapes that used it
    /// (Round/Circle/Diamond) don't need visually rounded corners to be
    /// distinguishable — `draw_shape` already marks Diamond with a `◇`
    /// corner glyph). No-ops below 2x2.
    pub(super) fn rect(&mut self, x: usize, y: usize, w: usize, h: usize) {
        if w < 2 || h < 2 {
            return;
        }
        let (x1, y1) = (x + w - 1, y + h - 1);
        self.hline(x + 1, x1 - 1, y, '─');
        self.hline(x + 1, x1 - 1, y1, '─');
        self.vline(x, y + 1, y1 - 1, '│');
        self.vline(x1, y + 1, y1 - 1, '│');
        self.set(x, y, '┌');
        self.set(x1, y, '┐');
        self.set(x, y1, '└');
        self.set(x1, y1, '┘');
    }

    /// Blanks a rectangle's interior (and border cells) to spaces, so a
    /// stale glyph from a differently-shaped previous draw can't bleed
    /// through (mdview `Canvas::clear_rect`, unstyled).
    pub(super) fn clear_rect(&mut self, x: usize, y: usize, w: usize, h: usize) {
        for yy in y..(y + h).min(self.h) {
            for xx in x..(x + w).min(self.w) {
                self.set(xx, yy, ' ');
            }
        }
    }

    /// Trims each row's trailing blanks and drops wide-char continuation
    /// markers, producing plain output lines.
    pub(super) fn into_lines(self) -> Vec<String> {
        let mut out = Vec::with_capacity(self.h);
        for y in 0..self.h {
            let row = &self.cells[y * self.w..(y + 1) * self.w];
            let end = row.iter().rposition(|&c| c != ' ' && c != CONT).map(|i| i + 1).unwrap_or(0);
            let line: String = row[..end].iter().filter(|&&c| c != CONT).collect();
            out.push(line);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(c: Canvas) -> Vec<String> {
        c.into_lines()
    }

    #[test]
    fn draws_box_with_text() {
        let mut c = Canvas::new(10, 3);
        c.set(0, 0, '┌');
        c.set(6, 0, '┐');
        c.set(0, 2, '└');
        c.set(6, 2, '┘');
        c.hline(1, 5, 0, '─');
        c.hline(1, 5, 2, '─');
        c.vline(0, 1, 1, '│');
        c.vline(6, 1, 1, '│');
        c.text(2, 1, "hi");
        assert_eq!(plain(c), vec!["┌─────┐", "│ hi  │", "└─────┘"]);
    }

    #[test]
    fn lines_merge_into_junctions() {
        let mut c = Canvas::new(5, 3);
        c.hline(0, 4, 1, '─');
        c.vline(2, 0, 2, '│');
        assert_eq!(plain(c), vec!["  │", "──┼──", "  │"]);
    }

    #[test]
    fn wide_chars_take_two_cells() {
        let mut c = Canvas::new(8, 1);
        c.text(0, 0, "한글");
        c.text(4, 0, "x");
        assert_eq!(plain(c), vec!["한글x"]);
    }

    #[test]
    fn overwriting_wide_char_clears_partner() {
        let mut c = Canvas::new(4, 1);
        c.text(0, 0, "한");
        c.set(1, 0, 'x');
        assert_eq!(plain(c), vec![" x"]);
    }
}
