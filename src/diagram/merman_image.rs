use crossterm::style::Color;
use merman::render::{
    HeadlessRenderer,
    raster::{RasterFitBox, RasterOptions},
};

use crate::theme::Theme;

use super::{DiagramError, RenderedMermaidImage, panic_payload_to_string};

const GENERATED_MERMAID_PREFIX: &str = "mdterm-generated://mermaid/";
const PREVIEW_CELL_WIDTH_PX: u32 = 12;
const MIN_PREVIEW_COLS: usize = 40;
const MAX_PREVIEW_COLS: usize = 160;
const MAX_PREVIEW_HEIGHT_PX: u32 = 900;
const RASTER_SCALE: f32 = 2.0;

pub(super) fn render(
    code: &str,
    theme: &Theme,
    width_cols: usize,
) -> Result<RenderedMermaidImage, DiagramError> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        render_inner(code, theme, width_cols)
    })) {
        Ok(result) => result,
        Err(payload) => Err(DiagramError::RenderFailed {
            message: panic_payload_to_string(payload),
        }),
    }
}

fn render_inner(
    code: &str,
    theme: &Theme,
    width_cols: usize,
) -> Result<RenderedMermaidImage, DiagramError> {
    let preview_cols = width_cols.clamp(MIN_PREVIEW_COLS, MAX_PREVIEW_COLS);
    let width_px = (preview_cols as u32).saturating_mul(PREVIEW_CELL_WIDTH_PX);
    let raster = RasterOptions::default()
        .with_fit_to(RasterFitBox::contain(width_px, MAX_PREVIEW_HEIGHT_PX))
        .with_scale(RASTER_SCALE)
        .with_background(color_to_css(theme.bg));
    let renderer = HeadlessRenderer::new()
        .with_strict_parsing()
        .with_diagram_id("mdterm-mermaid-image");

    let png = renderer
        .render_png_sync(code, &raster)
        .map_err(|err| DiagramError::RenderFailed {
            message: err.to_string(),
        })?
        .ok_or_else(|| DiagramError::ParseFailed {
            reason: "could not detect Mermaid diagram".to_string(),
        })?;

    if png.is_empty() {
        return Err(DiagramError::RenderFailed {
            message: "merman produced empty PNG output".to_string(),
        });
    }

    Ok(RenderedMermaidImage {
        key: generated_key(code, theme, width_cols),
        png,
    })
}

fn generated_key(code: &str, theme: &Theme, width_cols: usize) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    fn mix(hash: &mut u64, bytes: &[u8]) {
        for byte in bytes {
            *hash ^= u64::from(*byte);
            *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    mix(&mut hash, code.as_bytes());
    mix(&mut hash, theme.name().as_bytes());
    mix(&mut hash, width_cols.to_string().as_bytes());
    format!("{GENERATED_MERMAID_PREFIX}{hash:016x}")
}

fn color_to_css(color: Color) -> String {
    match color {
        Color::Rgb { r, g, b } => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Black => "#000000".to_string(),
        Color::White => "#ffffff".to_string(),
        _ => "#ffffff".to_string(),
    }
}
