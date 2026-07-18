use crate::style::StyledSpan;
use crate::theme::Theme;

mod canvas;
mod graph;
mod merman;
mod merman_image;
mod sequence;
mod theme;

// Cross-file reuse surface (spec: Architecture → Module decomposition).
// These re-exports are mandated by the spec for Phase B renderer modules;
// several are not yet consumed inside the crate, so allow unused until then.
#[allow(unused_imports)]
pub(crate) use canvas::{
    CONN_DOWN, CONN_LEFT, CONN_RIGHT, CONN_UP, Canvas, CanvasCell, CardDrawRow, EdgeEnd, EdgeStyle,
    NodeShape, junction_char,
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

#[derive(Debug, Clone)]
pub struct RenderedMermaidImage {
    pub key: String,
    pub png: Vec<u8>,
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
        // Keep mdterm-native renderers where merman ASCII does not currently
        // cover the diagram family.
        Some("graph")
        | Some("flowchart")
        | Some("stateDiagram")
        | Some("stateDiagram-v2")
        | Some("mindmap") => render_mermaid_native(code, theme),
        // Prefer merman for the families it supports, but preserve mdterm's
        // current native fallback for syntax/features merman rejects.
        Some("sequenceDiagram")
        | Some("classDiagram")
        | Some("classDiagram-v2")
        | Some("erDiagram") => match merman::render(code, theme) {
            Ok(rendered) => Ok(rendered),
            Err(merman_err) => render_mermaid_native(code, theme).or(Err(merman_err)),
        },
        Some("xychart") | Some("xychart-beta") => merman::render(code, theme),
        _ => render_mermaid_native(code, theme),
    }
}

pub fn render_mermaid_image(
    code: &str,
    theme: &Theme,
    width_cols: usize,
) -> Result<RenderedMermaidImage, DiagramError> {
    merman_image::render(code, theme, width_cols)
}

fn render_mermaid_native(
    code: &str,
    theme: &Theme,
) -> Result<(Vec<Vec<StyledSpan>>, usize), DiagramError> {
    let kw = first_diagram_keyword(code);
    match kw {
        Some("sequenceDiagram") => dispatch("sequenceDiagram", || sequence::render(code, theme)),
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
    fn merman_image_renderer_returns_png_and_stable_key() {
        let theme = Theme::dark();
        let src = "flowchart TD\nA[Start] --> B[Done]";

        let first = render_mermaid_image(src, &theme, 80).expect("first image render");
        let second = render_mermaid_image(src, &theme, 80).expect("second image render");

        assert_eq!(first.key, second.key);
        assert!(first.key.starts_with("mdterm-generated://mermaid/"));
        assert!(first.png.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]));
        assert!(!first.png.is_empty());
    }

    #[test]
    fn merman_image_key_changes_when_source_changes() {
        let theme = Theme::dark();
        let first =
            render_mermaid_image("flowchart TD\nA --> B", &theme, 80).expect("first image render");
        let second =
            render_mermaid_image("flowchart TD\nA --> C", &theme, 80).expect("second image render");

        assert_ne!(first.key, second.key);
    }

    #[test]
    fn merman_supported_xychart_renders() {
        let theme = Theme::dark();
        let src = "xychart-beta\n  title \"Sales\"\n  x-axis [Jan, Feb, Mar]\n  y-axis \"Revenue\" 0 --> 100\n  bar [20, 45, 80]";

        let (rows, width) = render_mermaid(src, &theme).expect("xychart should render via merman");
        let rendered = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|span| span.text.as_str())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(width > 0);
        assert!(rendered.contains("Sales"), "got:\n{rendered}");
        assert!(rendered.contains("Jan"), "got:\n{rendered}");
        assert!(rendered.contains("Revenue"), "got:\n{rendered}");
    }

    #[test]
    fn merman_failure_falls_back_to_native_class_renderer() {
        let theme = Theme::dark();
        let src = "classDiagram\n  class Order\n  class LineItem\n  Order \"1\" --> \"many\" LineItem : contains";

        let (rows, width) =
            render_mermaid(src, &theme).expect("native class renderer should remain fallback");
        let rendered = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|span| span.text.as_str())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(width > 0);
        assert!(rendered.contains("Order"), "got:\n{rendered}");
        assert!(rendered.contains("LineItem"), "got:\n{rendered}");
        assert!(rendered.contains("contains"), "got:\n{rendered}");
    }

    #[test]
    fn flowchart_dispatch_preserves_branch_node_labels() {
        let theme = Theme::dark();
        let src = "graph TD\n    A[Start] --> B{Decision}\n    B -->|Yes| C[Action 1]\n    B -->|No| D[Action 2]\n    C --> E[End]\n    D --> E";

        let (rows, width) = render_mermaid(src, &theme).expect("flowchart should render");
        let rendered = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|span| span.text.as_str())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(width > 0);
        assert!(rendered.contains("Action 1"), "got:\n{rendered}");
        assert!(rendered.contains("Action 2"), "got:\n{rendered}");
        assert!(rendered.contains("Decision"), "got:\n{rendered}");
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
}
