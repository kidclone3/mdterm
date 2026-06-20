use crate::theme::Theme;
use crossterm::style::Color;

pub(crate) fn edge_color(theme: &Theme, index: usize) -> Color {
    let colors = if theme.name() == "dark" {
        [
            Color::Rgb {
                r: 0,
                g: 215,
                b: 255,
            },
            Color::Rgb {
                r: 255,
                g: 176,
                b: 0,
            },
            Color::Rgb {
                r: 255,
                g: 95,
                b: 215,
            },
            Color::Rgb {
                r: 95,
                g: 255,
                b: 135,
            },
            Color::Rgb {
                r: 255,
                g: 95,
                b: 95,
            },
            Color::Rgb {
                r: 175,
                g: 135,
                b: 255,
            },
            Color::Rgb {
                r: 255,
                g: 255,
                b: 95,
            },
            Color::Rgb {
                r: 95,
                g: 175,
                b: 255,
            },
            Color::Rgb {
                r: 0,
                g: 255,
                b: 215,
            },
            Color::Rgb {
                r: 255,
                g: 135,
                b: 95,
            },
        ]
    } else {
        [
            Color::Rgb {
                r: 0,
                g: 92,
                b: 197,
            },
            Color::Rgb {
                r: 211,
                g: 86,
                b: 0,
            },
            Color::Rgb {
                r: 159,
                g: 0,
                b: 136,
            },
            Color::Rgb {
                r: 0,
                g: 115,
                b: 73,
            },
            Color::Rgb {
                r: 203,
                g: 36,
                b: 49,
            },
            Color::Rgb {
                r: 93,
                g: 63,
                b: 211,
            },
            Color::Rgb {
                r: 140,
                g: 104,
                b: 0,
            },
            Color::Rgb {
                r: 0,
                g: 118,
                b: 168,
            },
            Color::Rgb {
                r: 0,
                g: 128,
                b: 128,
            },
            Color::Rgb {
                r: 170,
                g: 70,
                b: 20,
            },
        ]
    };

    colors[index % colors.len()]
}
