# Mermaid Merman Image Rendering Design

## Goal

Render Mermaid fenced blocks as terminal images produced by `merman`'s Rust
raster pipeline, then display those images through mdterm's existing terminal
image protocols. In interactive terminals this gives higher-fidelity Mermaid
output without returning to `mermaid.ink`, shelling out to Node, or duplicating
Kitty/iTerm2/Sixel rendering logic.

## Background

mdterm currently renders Mermaid blocks as terminal text. The dispatch path is:

- `markdown::Renderer::emit_code_block` calls `diagram::render_mermaid`.
- `diagram::render_mermaid` uses mdterm-native renderers for graph/state/mindmap
  families, merman ASCII for several families, and merman-only ASCII for
  xychart.
- Failures emit a `mermaid (render error: ...)` banner and then show the source.

The image display side already exists in `src/image.rs`. Markdown images become
`LineMeta::Image` placeholder rows; the viewer fetches/decodes them
asynchronously and pre-renders them for Kitty, Kitty Unicode, iTerm2, Sixel,
Terminology, or half-block fallback.

`merman 0.7.0` includes a `raster` feature. Its public API supports
`merman::render::HeadlessRenderer::render_png_sync` with
`RasterOptions`, which internally renders resvg-safe SVG to PNG bytes.

## Scope

In scope:

- Use `merman` with features `ascii` and `raster`.
- Add a Mermaid render mode: `auto`, `image`, or `ascii`.
- In interactive viewer mode, make `auto` use merman-generated PNG images.
- Keep the current text renderer available as the `ascii` mode and as the
  failure fallback.
- Feed generated Mermaid PNG bytes into the existing `ImageCache` path, using an
  internal generated-image key rather than public local file paths.
- Update README/CLAUDE docs and tests that describe Mermaid behavior.

Out of scope:

- Reintroducing `mermaid.ink`.
- Shelling out to Mermaid CLI, Node, Chromium, or external renderers.
- Replacing mdterm's terminal image protocols.
- Removing mdterm-native Mermaid text renderers.
- Making HTML export inline generated PNGs in this change.

## User-Facing Behavior

`--mermaid-render` accepts:

- `auto`: default. Interactive terminal mode renders Mermaid as images. Piped
  output, `--no-color`, and export paths keep ASCII/source fallback behavior.
- `image`: interactive terminal mode prefers merman PNG images. Non-interactive
  output cannot display inline terminal graphics, so it falls back to ASCII/source
  with no failed process exit.
- `ascii`: current behavior. Mermaid renders as text diagrams where supported,
  or as an error banner plus source when unsupported or malformed.

The config file gets the same `mermaid_render = "auto" | "image" | "ascii"`
setting. CLI overrides config.

If merman image generation fails, mdterm shows the existing render-error banner
and the original Mermaid source. In `auto` mode the implementation first tries
the current ASCII renderer before showing the banner. In explicit `image` mode,
the banner names the merman raster failure if no image is produced.

## Architecture

### Render Mode Plumbing

Add a small enum in `markdown.rs` or a new render-options module:

```rust
pub enum MermaidRenderMode {
    Auto,
    Image,
    Ascii,
}
```

Keep the existing `markdown::render(...)` and `render_with(...)` wrappers for
tests, export, and piped output. Add an options-bearing entry point used by the
interactive viewer:

```rust
pub struct RenderOptions {
    pub line_numbers: bool,
    pub mermaid_render: MermaidRenderMode,
}
```

`main.rs` resolves the CLI/config mode. The interactive viewer passes that mode
to `ViewerOptions`. Piped and export paths always pass `Ascii` because stdout
and HTML export are not terminal image surfaces in this change.

### Merman Raster Adapter

Add a new adapter beside the existing ASCII adapter, for example
`src/diagram/merman_image.rs`.

The adapter:

- Builds a `HeadlessRenderer` with strict parsing and a stable diagram id.
- Calls `render_png_sync` with `RasterOptions`.
- Uses `RasterFitBox::contain(width_px, height_px)` with a deterministic bounded
  preview box derived from markdown render width plus a maximum preview height.
  The existing image pipeline still performs the final terminal-cell resize.
- Uses a theme-aware opaque background matching the mdterm theme background.
- Returns `Vec<u8>` PNG bytes or `DiagramError`.

The adapter must run under `catch_unwind`, matching the current renderer safety
contract.

### Generated Image Source

Do not write generated Mermaid PNGs to arbitrary local files and do not expose
absolute temp paths through the public image loader.

Instead, extend `ImageCache` with an internal generated-image registry:

```rust
pub fn insert_generated_png(&mut self, key: String, bytes: Vec<u8>);
```

The key is deterministic for a given diagram source, theme, and raster settings,
for example `mdterm-generated://mermaid/<hash>`. The cache decodes the bytes into
a `DynamicImage` and stores them in the existing `images` map. From that point
onward, all current pre-rendering, row calculation, Kitty upload, iTerm2 block
rendering, Sixel rendering, and half-block fallback behavior stays unchanged.

### Markdown Emission

For Mermaid blocks:

1. If mode is `Ascii`, use the current `diagram::render_mermaid` path.
2. If mode is `Auto` or `Image`, call the merman raster adapter.
3. On image success, emit a `LineMeta::Image` block with alt text like
   `mermaid diagram` and URL/key `mdterm-generated://mermaid/<hash>`.
4. Record the generated PNG bytes in `DocumentInfo` or a new render sidecar so
   `ViewerState::rebuild` can insert them into `ImageCache` before queuing image
   fetches.
5. On image failure, fall back to ASCII in `Auto`; if ASCII also fails, emit the
   current error banner plus source. In `Image`, show the raster error banner plus
   source.

`DocumentInfo` is the natural sidecar because it already carries render-time
metadata out of markdown rendering. Add:

```rust
pub struct GeneratedImage {
    pub key: String,
    pub png: Vec<u8>,
}

pub struct DocumentInfo {
    pub code_blocks: Vec<CodeBlockContent>,
    pub frontmatter_lines: Option<usize>,
    pub generated_images: Vec<GeneratedImage>,
}
```

The viewer inserts these images into `ImageCache` immediately after markdown
rendering and before scanning `LineMeta::Image` rows. This avoids background
network fetches for generated Mermaid images.

## Error Handling

- A merman parse error is a render failure, not a crash.
- A merman raster panic is caught and converted to `DiagramError::RenderFailed`.
- Empty PNG output or undecodable PNG bytes emit a render-error banner plus
  source.
- Large diagrams use merman raster size limits and mdterm's existing image
  downscaling limits.
- Generated image keys are internal and are never fetched over HTTP.

## Testing

Add focused tests for:

- `--mermaid-render ascii` preserves current Mermaid text behavior.
- Interactive/options render path emits `LineMeta::Image` plus a generated image
  sidecar for a simple flowchart.
- `auto` falls back to ASCII/source when image generation fails.
- Piped/plain rendering does not emit generated image placeholders by default.
- Generated image keys are stable for identical source/theme/settings and differ
  when source changes.
- `ViewerState::rebuild` inserts generated images before image queue scanning,
  so generated Mermaid images are not sent to the HTTP/local-path fetcher.

Run at minimum:

```sh
cargo test diagram::
cargo test markdown::
cargo test image::
cargo test
```

If build time becomes high after enabling `merman/raster`, keep the first loop
targeted during development and run the full suite before completion.

## Docs

Update README and CLAUDE docs to say:

- Mermaid image rendering is local and powered by merman raster output.
- Terminal display still depends on the existing image protocol detection:
  Kitty, Kitty Unicode, iTerm2, Sixel, Terminology, or half-block fallback.
- `--mermaid-render ascii` is available for text-only rendering.
- Unsupported or malformed diagrams show a render-error banner and source.
