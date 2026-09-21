# spec-viewer

A terminal UI (`m`) for browsing `.kiro/` spec directories: a tree panel of
specs (with phase/progress badges) next to a rendered-markdown document
panel, live-updated as files change on disk.

Built for the [Kiro-style spec-driven development](https://github.com/hodorii/agentic-psdd)
workflow, but the `--all` mode drops the `.kiro` requirement entirely and
browses any directory of markdown files, so it also works as a general
terminal markdown viewer with mermaid diagram support.

## Install

Requires a Rust toolchain (`cargo`).

```sh
cargo install --path . --root ~/.local   # or: make install
```

`make run` builds and runs against the parent directory (handy when this
crate is checked out a level under a project's `.kiro/`); `make install-remote
HOST=<ssh-host>` rsyncs the source to a remote host and builds it there.

## Usage

```sh
m [path]                 # find the nearest .kiro root at/above `path` (default: cwd)
m --all [dir]             # browse any markdown directory, no .kiro required
```

Flags:
- `--tree <auto|always|hidden|single>` — tree panel visibility mode (default `auto`)
- `--sort <name|phase|updated|progress>` — initial spec-tree sort key
- `--no-watch` — disable filesystem watching
- `--log <path>` — write a log file (rejected if it resolves inside the `.kiro` root)
- `--diagram-engine <dg|mdview|builtin>` — mermaid graph renderer (see below)

Keys: `q` quit · `Tab` switch panel · `j`/`k` line scroll · `d`/`u` half-page ·
`f`/`PageDown`, `b`/`PageUp` full page · `g`/`Home`, `G`/`End` top/bottom ·
`[`/`]` prev/next heading · `/` search, `n`/`N` next/prev match · `t` table of
contents · `T` toggle tree panel · `1`–`4` layout modes (auto/fold/expand/single) ·
`s` cycle sort key · `?` help · arrow keys for tree/document navigation.

## Diagram engines

Mermaid flowchart/state diagrams in rendered markdown go through a pluggable
`GraphEngine`:
- `dg` (default, feature `engine-dg`) — renders via [dg](https://github.com/hodorii/dg) (MIT).
- `mdview` — a band-routed layout engine ported from [mdview](https://github.com/hjshim/mdview) (MIT); see `THIRD_PARTY.md`.
- `builtin` — a simpler shared-vertical-bus layout, no external dependency.
- `graphs-tui` (feature `engine-graphs-tui`, **not enabled by default**) — routes through the [graphs-tui](https://crates.io/crates/graphs-tui) crate, licensed AGPL-3.0-or-later; a binary built with this feature carries AGPL obligations. See `THIRD_PARTY.md`.

## License

MIT (`LICENSE`), except the optional `engine-graphs-tui` feature which pulls
in an AGPL-3.0-or-later dependency — not part of the default build. Third-party
attributions: `THIRD_PARTY.md`.
