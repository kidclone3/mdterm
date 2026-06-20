# mdterm

A terminal-based Markdown viewer written in Rust. Renders Markdown files with syntax highlighting, styled formatting, and interactive navigation.

## Screenshots

| | |
|---|---|
| ![Demo](screenshots/demo.png) | ![Light Theme](screenshots/light.png) |
| ![Math Rendering](screenshots/math.png) | ![Mermaid Diagrams](screenshots/mermaid.png) |
| ![Search](screenshots/search.png) | |

## Features

- **Interactive TUI** — Scroll, navigate with keyboard and mouse
- **Syntax highlighting** — Code blocks highlighted via syntect (base16-ocean.dark / InspiredGitHub themes)
- **Rich formatting** — Headings, bold, italic, strikethrough, lists, blockquotes, tables, interactive checkboxes
- **Inline images** — Renders images in the terminal via Kitty, iTerm2, or Unicode half-block fallback
- **Clickable links** — OSC 8 hyperlinks in supporting terminals
- **In-document search** — `/` to search with regex support, `n`/`N` to jump between matches
- **Table of contents** — Press `o` to browse and jump to any heading
- **Fuzzy heading search** — Press `:` to filter headings by name
- **Heading jumps** — `[` / `]` to jump between sections
- **Local file links** — Click or select relative markdown links to navigate between files, with `Backspace` to go back
- **Link picker** — Press `f` to list all links, type a number to open in browser
- **Click-to-copy** — Click any heading section, list, or code block to copy it; `Y` copies full document, `c` copies nearest code block
- **Mermaid diagrams** — Renders every mermaid diagram type (flowchart, sequence, class, state, gantt, pie, er, …) as a real image via [mermaid.ink](https://mermaid.ink), displayed through the terminal image pipeline (Kitty / iTerm2 / Sixel / half-block). Falls back to native ASCII box-drawing for flowcharts when offline or when stdout is piped.
- **Math rendering** — LaTeX to Unicode: `$\alpha + \beta$` renders as `α + β`
- **Slide mode** — `--slides` treats `---` as slide separators for terminal presentations
- **Auto-reload** — Automatically detects file changes and reloads (via inotify/FSEvents/kqueue)
- **Stdin support** — Pipe markdown from any command: `curl ... | mdterm`
- **Multiple files** — `mdterm a.md b.md`, switch with `Tab` / `Shift+Tab`
- **HTML export** — `--export html` outputs themed, self-contained HTML
- **Dark/light themes** — Toggle with `t`, or set via `--theme` / config file
- **Line numbers** — Toggle with `L` for code blocks
- **Config file** — `~/.config/mdterm/config.toml` for persistent preferences
- **Word wrapping** — Responsive re-wrapping on terminal resize
- **JSON viewer** — Render JSON files with syntax-colored keys, values, and structure
- **Part-of-speech highlighting** *(optional feature)* — Color prose words by part of speech (noun, verb, adjective, …) with a toggle, powered by a vendored averaged-perceptron tagger. Enable at install time; see below.
- **Pipe-friendly** — Outputs plain styled text when stdout is piped

## Installation

Requires Rust 1.85+ (edition 2024).

```bash
cargo install --path .
```

### Optional: POS highlighting

The `pos` feature adds part-of-speech coloring (~2 MB of embedded model data). It
is off by default to keep the default build lean.

```bash
cargo install --features pos --path .    # with POS highlighting
cargo install --path .                   # without (default)
```

## Usage

```bash
mdterm README.md                    # view a file
mdterm a.md b.md                    # multiple files (Tab to switch)
mdterm data.json                    # view a JSON file
cat README.md | mdterm              # read from stdin
mdterm --slides deck.md             # slide mode
mdterm --export html doc.md > out.html  # export to HTML
mdterm --theme light README.md      # light theme
mdterm -l README.md                 # line numbers in code blocks
```

When piped, mdterm outputs styled text without the interactive viewer:

```bash
mdterm README.md | less -R
```

## Controls

### Navigation

| Key | Action |
|-----|--------|
| `j` / `Down` | Scroll down one line |
| `k` / `Up` | Scroll up one line |
| `h` / `Left` | Pan left |
| `l` / `Right` | Pan right |
| `Space` / `Page Down` | Page down |
| `b` / `Page Up` | Page up |
| `d` / `u` (or `Ctrl+d` / `Ctrl+u`) | Half-page down / up |
| `g` / `Home` | Jump to top |
| `G` / `End` | Jump to bottom |
| `[` / `]` | Previous / next heading |
| `Backspace` | Go back (after following a local file link) |
| Mouse scroll | Scroll up/down |

### Search

| Key | Action |
|-----|--------|
| `/` | Open search (supports regex) |
| `Enter` | Execute search |
| `n` / `N` | Next / previous match |
| `Esc` | Clear search |

### Features

| Key | Action |
|-----|--------|
| `o` | Table of contents overlay |
| `:` | Fuzzy heading search |
| `f` | Link picker (open URLs / follow local links) |
| `t` | Toggle dark/light theme |
| `L` | Toggle line numbers in code blocks |
| `P` | Toggle part-of-speech highlighting (requires `pos` feature) |
| Click heading | Copy heading section to clipboard |
| Click list | Copy entire list to clipboard |
| Click code block | Copy code block to clipboard |
| `Y` | Copy entire document to clipboard |
| `c` | Copy nearest code block to clipboard |
| `Tab` / `Shift+Tab` | Switch between files |
| `?` / `F1` | Help screen |
| `q` / `Ctrl+C` | Quit |

### Slide Mode (`--slides`)

| Key | Action |
|-----|--------|
| `Right` / `Space` / `l` / `j` / `Down` / `Page Down` | Next slide |
| `Left` / `b` / `h` / `k` / `Up` / `Page Up` | Previous slide |
| `g` / `Home` | First slide |
| `G` / `End` | Last slide |

## Configuration

Create `~/.config/mdterm/config.toml`:

```toml
theme = "dark"          # "dark" or "light"
line_numbers = false     # show line numbers in code blocks
width = 0               # display width (0 = auto)

[pos]                   # part-of-speech highlighting (requires `pos` feature)
enabled = false          # start with POS on
categories = ["noun", "verb"]  # only these; omit or "all" for every category
```

CLI flags override config file settings.

## CLI Reference

```
mdterm [OPTIONS] [FILES]...

Arguments:
  [FILES]...               Markdown file(s) to view

Options:
  -T, --theme <THEME>      Theme: dark or light
  -w, --width <WIDTH>      Display width override (0 = auto)
  -s, --slides             Slide mode (--- as slide separators)
  -l, --line-numbers       Show line numbers in code blocks
      --export <FORMAT>    Export format (html)
      --no-color           Disable colors
      --pos [CATEGORIES]  Part-of-speech highlighting (e.g. --pos noun,verb; needs `pos` feature)
  -h, --help               Print help
  -V, --version            Print version
```

## Building

```bash
cargo build --release
```

## Demo

![Demo](demo.gif)

## Third-Party Assets

The optional `pos` feature vendors:

- **Averaged-perceptron tagger logic** adapted from
  [`postagger.rs`](https://github.com/shubham0204/postagger.rs) (Apache-2.0).
- **Pretrained model** (`averaged_perceptron_tagger`) from
  [NLTK `nltk_data`](https://github.com/nltk/nltk_data) (Apache-2.0),
  redistributed in `pos_model/`.

Both are Apache-2.0 licensed and compatible with mdterm's MIT license.

## License

MIT
