use unicode_width::UnicodeWidthStr;

use crate::style::{Style, StyledSpan};
use crate::theme::Theme;

use super::{DiagramError, panic_payload_to_string};

pub(super) fn render(
    code: &str,
    theme: &Theme,
) -> Result<(Vec<Vec<StyledSpan>>, usize), DiagramError> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render_inner(code, theme))) {
        Ok(result) => result,
        Err(payload) => Err(DiagramError::RenderFailed {
            message: panic_payload_to_string(payload),
        }),
    }
}

fn render_inner(code: &str, theme: &Theme) -> Result<(Vec<Vec<StyledSpan>>, usize), DiagramError> {
    let options = merman::ascii::AsciiRenderOptions::unicode();
    let renderer = merman::ascii::HeadlessAsciiRenderer::new()
        .with_strict_parsing()
        .with_ascii_options(options);

    let rendered = renderer
        .render_ascii_sync(code)
        .map_err(|err| DiagramError::ParseFailed {
            reason: err.to_string(),
        })?
        .ok_or_else(|| DiagramError::ParseFailed {
            reason: "could not detect Mermaid diagram".to_string(),
        })?;

    text_to_rows(&rendered, theme)
}

fn text_to_rows(
    rendered: &str,
    theme: &Theme,
) -> Result<(Vec<Vec<StyledSpan>>, usize), DiagramError> {
    let rows: Vec<Vec<StyledSpan>> = rendered
        .lines()
        .map(|line| {
            vec![StyledSpan {
                text: line.to_string(),
                style: Style {
                    fg: Some(theme.fg),
                    ..Default::default()
                },
            }]
        })
        .collect();

    if rows.is_empty() {
        return Err(DiagramError::ParseFailed {
            reason: "merman produced empty output".to_string(),
        });
    }

    let width = rendered
        .lines()
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);

    Ok((rows, width))
}
