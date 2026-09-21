# Third-Party Notices

## mdview

Portions of this crate's rendering code (table borders, heading/emphasis
theme structure, and the box-drawing canvas used for mermaid diagram edges)
were adapted from [mdview](https://github.com/aaron-shim/mdview) by hjshim,
used under the MIT License.

```
MIT License

Copyright (c) 2026 hjshim

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

Adapted from (source files, at the revision reviewed on 2026-09-14):
- `src/theme.rs` — heading/emphasis/table/diagram style palette structure (task 9.3).
- `src/render/table.rs` — table border/grid layout algorithm (task 9.4).
- `src/render/canvas.rs` — directional-bitmask box-drawing canvas for compositing diagram edges/junctions (task 9.5).
- `src/hangul.rs` — Hangul jamo composition (NFD → NFC) algorithm (task 10.3).
- `src/render/mermaid/graph.rs` — layered (Sugiyama-style) graph layout:
  subgraph-cluster-aware ranking, barycenter ordering, virtual-node long-edge
  splitting, band-routed edge wiring, and a dedicated feedback-edge gutter
  (task 14.2, `markdown/mermaid/engine/mdview.rs` — the default
  `--diagram-engine`, replacing `builtin`'s shared-vertical-bus wiring per
  requirement 5.20).

None of the above files were copied verbatim; their approach was ported into
this crate's own `markdown::{Line, Span, SpanStyle}` types and module
structure. `src/hangul.rs` is the closest to verbatim (the composition
algorithm itself is a direct Unicode-standard implementation with little room
for variation); the one behavioral-preserving change is that this crate's
`markdown/hangul.rs` rewrites mdview's edition-2024 "let-chains" as nested
`if let` (this crate's `Cargo.toml` uses `edition = "2021"`).

`engine/mdview.rs`'s port of `graph.rs` drops `Theme`/`Style`/styled `Line`
entirely (this crate's mermaid output is plain, unstyled `Vec<String>`).
An earlier revision (task 14.2/14.5) added LR direction support that
mdview's original doesn't have (mdview always lays out top-to-bottom by
design); that addition was removed (task 16.3) after a real side-by-side
comparison against the reference `mdview` binary showed the transposed
connector drawing producing corner-glyph overlaps and stray `┼` crossings
that the original TB-only wiring does not have — this port now matches
mdview's original TB-only behavior exactly, ignoring a declared `LR`
direction just like upstream does (`--diagram-engine builtin` is the
engine that renders LR). Rounded corners (`╭╮╰╯`) were dropped in favor of
square ones — this crate's box-glyph merge table (`canvas.rs`, ported for
task 9.5) doesn't cover them, and losing the rounding doesn't affect which
shapes are distinguishable.
