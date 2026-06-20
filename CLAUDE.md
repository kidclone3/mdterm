# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

mdterm is a terminal-based Markdown viewer written in Rust. It renders Markdown files with syntax highlighting, styled formatting, and interactive navigation (scrolling, keyboard/mouse controls). When stdout is piped, it outputs plain styled text instead of the interactive TUI.

## Build Commands

```bash
cargo build              # debug build
cargo build --release    # release build
cargo run -- <file.md>   # run with a markdown file
cargo check              # type-check without building
cargo clippy             # lint
cargo fmt                # format code
cargo test               # run tests
```

## Architecture

Ten source files in `src/`:

- **main.rs** — Entry point. Uses `clap` for CLI arg parsing, handles stdin/file input, dispatches to viewer (TTY), piped output, or HTML export.
- **markdown.rs** — Stateful markdown renderer. Processes `pulldown-cmark` events into `(Vec<Line>, DocumentInfo)`. Handles syntax highlighting, math rendering (LaTeX→Unicode), image placeholders, line numbers, mermaid diagram rendering, and metadata tracking (headings, code blocks, slide breaks).
- **style.rs** — Data types (`Style`, `StyledSpan`, `Line`, `LineMeta`, `DocumentInfo`) and word-wrapping logic. `LineMeta` tracks heading/code-block/slide metadata through wrapping.
- **viewer.rs** — Interactive TUI with multiple view modes (Normal, Search, TOC, LinkPicker, FuzzyHeading). Supports slide mode, auto-reload on file changes (via notify crate), multi-file switching, clipboard operations, regex search, overlay panels, and image rendering.
- **theme.rs** — Two complete themes (dark/light) with 40+ color fields including overlay, math, image, and line number colors.
- **config.rs** — Loads `~/.config/mdterm/config.toml` for persistent settings (theme, line_numbers, width).
- **export.rs** — HTML export with inline CSS matching the current theme.
- **image.rs** — Terminal image rendering with three protocols: Kitty (ID-based upload/placement), iTerm2 (inline image sequences), and Unicode half-block fallback. Fetches images on background threads via `std::sync::mpsc` (non-blocking — `start_fetch()` spawns a thread per URL, `poll_completed()` drains results each event-loop tick). Also handles downscaling, caching, and terminal cell metric detection.
- **json.rs** — JSON file viewer. Parses JSON and renders it with semantic coloring (keys, strings, numbers, booleans, nulls) and indented structure.
- **diagram/** — Mermaid rendering module tree. Public API `render_mermaid()` returns `Result<_, DiagramError>` and dispatches by diagram keyword; each renderer runs under `catch_unwind` so a parse error or panic shows an inline error banner plus the original source instead of crashing the TUI. Sub-modules:
  - `mod.rs` — dispatch + `is_unsupported_diagram` (still-pending types) + `mermaid_image_url` (used only for unported types until Batch R).
  - `canvas.rs` — `Canvas` (character grid), `CanvasCell`, `draw_node` / `draw_node_with_height` / `draw_card`, `draw_edge_td` / `draw_edge_lr` (dispatch on `EdgeStyle { dashed, head, tail, label, far_label }`), `draw_crowsfoot`, `draw_tree_edge`, `junction_char`, `to_span_rows`. The `NodeShape` enum covers Rectangle, Rounded, Diamond, Circle, Final (ringed dot for stateDiagram), ForkBar (stateDiagram).
  - `theme.rs` — `edge_color()` palette + per-family color tables.
  - `sequence.rs` — `sequenceDiagram` parser + renderer (participants, messages, notes, self-loops, blocks).
  - `graph/mod.rs` — shared layout helpers: `assign_layers` (Kahn topological sort), `order_within_layers` (barycenter heuristic), `refine_lr_layer_order` (adjacent-swap refinement), `NodeLayout`, LR port/lane maps.
  - `graph/flowchart.rs` — `graph` / `flowchart` parser + TD/LR renderers ( reused as the layout backend by stateDiagram).
  - `graph/state.rs` — `stateDiagram` / `stateDiagram-v2` (composite states via sub-canvas stamping, fork/join bars, initial/final pseudo-states, notes).
  - `graph/class.rs` — `classDiagram` / `classDiagram-v2` (class cards with visibility markers, generics, stereotypes, UML relationships with hollow-triangle / diamond edge heads).
  - `graph/er.rs` — `erDiagram` (entity cards with PK/FK badges, crow's-foot cardinality endpoints).
  - `graph/mindmap.rs` — `mindmap` (indentation-based tree, radial left/right layout, orthogonal tree edges).

**Data flow:** markdown text → `pulldown-cmark` events → `Renderer` (markdown.rs) → `(Vec<Line>, DocumentInfo)` → `wrap_lines` (style.rs) → terminal/HTML output

## Key Dependencies

- **pulldown-cmark 0.11** — CommonMark parser (events/AST, math support)
- **crossterm 0.28** — Terminal control (raw mode, colors, events)
- **syntect 5** — Syntax highlighting for code blocks
- **clap 4** — CLI argument parsing
- **regex 1** — Regex search support
- **open 5** — Open URLs in browser (link picker)
- **serde + toml** — Config file parsing
- **dirs 5** — Platform config directory lookup
- **image 0.25** — Image loading and processing (PNG, JPEG, GIF, WebP, BMP, ICO, TIFF)
- **libc 0.2** — Unix FFI for terminal cell pixel metrics (ioctl TIOCGWINSZ)
- **base64 0.22** — Base64 encoding for image protocol escape sequences
- **ureq 3** — Pure-Rust HTTP client for fetching remote images (replaces shelling out to `curl`)
- **serde_json 1** — JSON parsing for the JSON file viewer
- **notify 7** — Cross-platform filesystem watcher (inotify/FSEvents/kqueue) for auto-reload

## Rust Edition

Uses Rust edition 2024 (requires rustc 1.85+).
