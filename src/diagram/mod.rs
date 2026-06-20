use crate::style::StyledSpan;
use crate::theme::Theme;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

mod canvas;
mod graph;
mod sequence;
mod theme;

// Cross-file reuse surface (spec: Architecture → Module decomposition).
// These re-exports are mandated by the spec for Phase B renderer modules;
// several are not yet consumed inside the crate, so allow unused until then.
#[allow(unused_imports)]
pub(crate) use canvas::{
    junction_char, Canvas, CanvasCell, CardDrawRow, EdgeEnd, EdgeStyle, NodeShape, CONN_DOWN,
    CONN_LEFT, CONN_RIGHT, CONN_UP,
};
#[allow(unused_imports)]
pub(crate) use graph::NodeLayout;

// ───── Errors ─────

/// Why a native mermaid render failed. Carried out of `render_mermaid` so the
/// caller (markdown dispatcher) can show a labelled banner naming the failure
/// mode and then fall through to the raw source block.
#[derive(Debug)]
pub enum DiagramError {
    /// The parser could not make sense of the source (malformed input,
    /// unsupported syntax within a type, empty body, or an unported type).
    ParseFailed { reason: String },
    /// The parser succeeded but the renderer panicked during layout or canvas
    /// drawing (a bug in the renderer, or pathological input). Caught via
    /// `catch_unwind` so one bad diagram cannot kill the TUI.
    RenderFailed { message: String },
}

impl DiagramError {
    /// User-visible reason text, regardless of variant.
    pub fn reason(&self) -> &str {
        match self {
            DiagramError::ParseFailed { reason } => reason,
            DiagramError::RenderFailed { message } => message,
        }
    }
}

/// Extract a best-effort string from a panic payload (`Box<dyn Any + Send>`).
fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "renderer panicked".to_string()
}

/// Run a renderer closure, converting its `Option` result and any panic into
/// the public `Result<_, DiagramError>` shape. `keyword` names the diagram
/// type for the parse-failure reason (e.g. "could not parse sequenceDiagram").
fn dispatch<F>(keyword: &str, f: F) -> Result<(Vec<Vec<StyledSpan>>, usize), DiagramError>
where
    F: FnOnce() -> Option<(Vec<Vec<StyledSpan>>, usize)>,
{
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Some(rendered)) => Ok(rendered),
        Ok(None) => Err(DiagramError::ParseFailed {
            reason: format!("could not parse {keyword}"),
        }),
        Err(payload) => Err(DiagramError::RenderFailed {
            message: panic_payload_to_string(payload),
        }),
    }
}

// ───── Dispatch ─────

/// First non-empty, non-comment token — the mermaid diagram type keyword.
pub(crate) fn first_diagram_keyword(code: &str) -> Option<&str> {
    code.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("%%"))
        .map(|l| l.split_whitespace().next().unwrap_or(l))
}

/// Diagram types we don't render natively yet — these fall back to showing the
/// raw mermaid source as a code block rather than being garbled by the
/// flowchart parser.
pub(crate) fn is_unsupported_diagram(kw: &str) -> bool {
    matches!(
        kw,
        "journey"
            | "gantt"
            | "pie"
            | "gitGraph"
            | "timeline"
            | "quadrantChart"
            | "requirementDiagram"
            | "sankey"
            | "sankey-beta"
            | "xychart"
            | "xychart-beta"
            | "block"
            | "block-beta"
            | "packet"
            | "packet-beta"
            | "architecture"
            | "architecture-beta"
            | "C4Context"
            | "C4Container"
            | "C4Component"
            | "C4Dynamic"
            | "C4Deployment"
            | "zenuml"
            | "kanban"
            | "radar"
            | "radar-beta"
    )
}

/// Try to render mermaid code as a visual diagram.
///
/// Returns the rendered span rows and canvas width on success. On failure a
/// `DiagramError` explains whether parsing or rendering went wrong; the caller
/// is expected to show the error and fall back to the raw source block.
///
/// Each renderer runs under `catch_unwind` so a panic in new (Phase B) code is
/// contained and surfaces as `RenderFailed` rather than crashing the TUI.
pub fn render_mermaid(
    code: &str,
    theme: &Theme,
) -> Result<(Vec<Vec<StyledSpan>>, usize), DiagramError> {
    let kw = first_diagram_keyword(code);
    match kw {
        Some("sequenceDiagram") => {
            dispatch("sequenceDiagram", || sequence::render(code, theme))
        }
        Some("stateDiagram") | Some("stateDiagram-v2") => {
            dispatch("stateDiagram", || graph::state::render(code, theme))
        }
        Some("classDiagram") | Some("classDiagram-v2") => {
            dispatch("classDiagram", || graph::class::render(code, theme))
        }
        Some("erDiagram") => dispatch("erDiagram", || graph::er::render(code, theme)),
        Some("mindmap") => dispatch("mindmap", || graph::mindmap::render(code, theme)),
        Some(k) if is_unsupported_diagram(k) => Err(DiagramError::ParseFailed {
            reason: format!("could not parse {k}"),
        }),
        _ => dispatch("flowchart", || graph::flowchart::render(code, theme)),
    }
}

/// Build a mermaid.ink image URL for the given mermaid source.
///
/// The source is URL-safe base64-encoded (no padding) and appended to the
/// `https://mermaid.ink/img/` endpoint, which renders it server-side via the
/// real mermaid.js and returns a PNG. This covers every mermaid diagram type
/// (flowchart, sequence, class, state, gantt, pie, er, …) — not just the subset
/// handled by the native ASCII renderer.
///
/// `type=png` selects a transparent-background PNG so the image blends with the
/// terminal background, and `theme` mirrors mdterm's current theme so node/text
/// colors stay legible on dark or light backgrounds.
pub fn mermaid_image_url(code: &str, theme: &Theme) -> String {
    let encoded = URL_SAFE_NO_PAD.encode(code.as_bytes());
    let mermaid_theme = if theme.name() == "dark" {
        "dark"
    } else {
        "default"
    };
    format!("https://mermaid.ink/img/{encoded}?type=png&theme={mermaid_theme}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn unsupported_diagram_falls_back_to_source() {
        let theme = Theme::dark();
        assert!(render_mermaid("pie\n    \"A\" : 1", &theme).is_err());
    }

    #[test]
    fn flowchart_still_renders_after_dispatch() {
        let theme = Theme::dark();
        assert!(render_mermaid("graph TD\nA[Start] --> B[End]", &theme).is_ok());
        assert!(render_mermaid("flowchart LR\nA --> B", &theme).is_ok());
    }

    #[test]
    fn dispatch_passes_through_successful_render() {
        let result: Result<(Vec<Vec<StyledSpan>>, usize), DiagramError> =
            dispatch("flowchart", || Some((vec![vec![]], 42)));
        match result {
            Ok((_rows, w)) => assert_eq!(w, 42),
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_converts_parse_none_to_parse_failed() {
        let result: Result<(Vec<Vec<StyledSpan>>, usize), DiagramError> =
            dispatch("classDiagram", || None);
        match result {
            Err(DiagramError::ParseFailed { reason }) => {
                assert!(
                    reason.contains("classDiagram"),
                    "reason should name the keyword: {reason}"
                );
            }
            other => panic!("expected ParseFailed, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_catches_renderer_panic_as_render_failed() {
        let result: Result<(Vec<Vec<StyledSpan>>, usize), DiagramError> =
            dispatch("test", || panic!("synthetic boom"));
        match result {
            Err(DiagramError::RenderFailed { message }) => {
                assert!(
                    message.contains("synthetic boom"),
                    "panic payload should be preserved: {message}"
                );
            }
            other => panic!("expected RenderFailed, got {other:?}"),
        }
    }

    #[test]
    fn mermaid_image_url_encodes_source_and_dark_theme() {
        let url = mermaid_image_url("graph TD\nA-->B", &Theme::dark());
        // URL-safe base64 of "graph TD\nA-->B" has no padding/+/ .
        assert!(url.starts_with("https://mermaid.ink/img/"), "{url}");
        assert!(url.contains("?type=png"), "{url}");
        assert!(url.contains("theme=dark"), "{url}");
        // No standard base64 characters that would break a URL path.
        let path = url
            .split("/img/")
            .nth(1)
            .unwrap()
            .split('?')
            .next()
            .unwrap();
        assert!(
            !path.contains(['+', '/', '=']),
            "expected URL-safe base64, got {path}"
        );
    }

    #[test]
    fn mermaid_image_url_uses_default_theme_for_light() {
        let url = mermaid_image_url("sequenceDiagram\nA->>B: Hi", &Theme::light());
        assert!(url.contains("theme=default"), "{url}");
        assert!(url.contains("?type=png"), "{url}");
    }
}
