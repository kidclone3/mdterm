use crossterm::style::Color;

#[derive(Clone)]
pub struct Theme {
    // Main background / foreground
    pub bg: Color,
    pub fg: Color,

    // Frame / chrome
    pub border: Color,
    pub title: Color,
    pub position: Color,
    pub help_hint: Color,
    pub scrollbar_track: Color,
    pub scrollbar_thumb: Color,

    // Headings
    pub h1: Color,
    pub h2: Color,
    pub h3: Color,
    pub h4: Color,
    pub h5: Color,
    pub h6: Color,
    pub heading_separator: Color,

    // Code blocks
    pub code_bg: Color,
    pub code_border: Color,
    pub code_label: Color,
    pub syntect_theme: &'static str,
    pub diagram_error_fg: Color,

    // Inline code
    pub inline_code_fg: Color,
    pub inline_code_bg: Color,
    pub inline_code_tick: Color,

    // Blockquote
    pub blockquote_bar: Color,

    // Links
    pub link: Color,
    pub link_url: Color,

    // Lists
    pub bullet: Color,
    pub task_done: Color,
    pub task_pending: Color,

    // Rules
    pub rule: Color,

    // Tables
    pub table_border: Color,
    pub table_header: Color,

    // Search
    pub search_prompt: Color,
    pub search_match_bg: Color,
    pub search_current_bg: Color,
    pub search_current_fg: Color,
    pub search_no_match: Color,

    // Overlays (TOC, link picker, fuzzy search)
    pub overlay_bg: Color,
    pub overlay_border: Color,
    pub overlay_selected_bg: Color,
    pub overlay_selected_fg: Color,
    pub overlay_text: Color,
    pub overlay_muted: Color,

    // Images
    pub image_fg: Color,

    // Slide mode
    pub slide_indicator: Color,

    // Math
    pub math_fg: Color,

    // Line numbers
    pub line_number: Color,

    // JSON
    pub json_key: Color,
    pub json_string: Color,
    pub json_number: Color,
    pub json_bool: Color,
    pub json_null: Color,
    pub json_bracket: Color,
    pub json_path: Color,
    pub json_focus_bg: Color,

    // classDiagram member visibility
    pub member_plus: Color,
    pub member_minus: Color,
    pub member_hash: Color,
    pub member_tilde: Color,

    // stateDiagram composite state background tint
    pub composite_state_bg: Color,

    // erDiagram key badge colors
    pub key_badge_pk: Color,
    pub key_badge_fk: Color,

    is_dark: bool,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            is_dark: true,

            bg: Color::Rgb {
                r: 30,
                g: 30,
                b: 46,
            },
            fg: Color::Rgb {
                r: 205,
                g: 214,
                b: 244,
            },

            border: Color::Rgb {
                r: 68,
                g: 71,
                b: 90,
            },
            title: Color::Rgb {
                r: 147,
                g: 153,
                b: 178,
            },
            position: Color::Rgb {
                r: 108,
                g: 112,
                b: 134,
            },
            help_hint: Color::Rgb {
                r: 88,
                g: 91,
                b: 112,
            },
            scrollbar_track: Color::Rgb {
                r: 49,
                g: 50,
                b: 68,
            },
            scrollbar_thumb: Color::Rgb {
                r: 127,
                g: 132,
                b: 156,
            },

            h1: Color::Rgb {
                r: 205,
                g: 214,
                b: 244,
            },
            h2: Color::Rgb {
                r: 137,
                g: 180,
                b: 250,
            },
            h3: Color::Rgb {
                r: 203,
                g: 166,
                b: 247,
            },
            h4: Color::Rgb {
                r: 166,
                g: 227,
                b: 161,
            },
            h5: Color::Rgb {
                r: 249,
                g: 226,
                b: 175,
            },
            h6: Color::Rgb {
                r: 127,
                g: 132,
                b: 156,
            },
            heading_separator: Color::Rgb {
                r: 49,
                g: 50,
                b: 68,
            },

            code_bg: Color::Rgb {
                r: 30,
                g: 32,
                b: 42,
            },
            code_border: Color::Rgb {
                r: 68,
                g: 71,
                b: 90,
            },
            code_label: Color::Rgb {
                r: 108,
                g: 112,
                b: 134,
            },
            diagram_error_fg: Color::Rgb {
                r: 237,
                g: 105,
                b: 97,
            },
            syntect_theme: "base16-ocean.dark",

            inline_code_fg: Color::Rgb {
                r: 242,
                g: 205,
                b: 147,
            },
            inline_code_bg: Color::Rgb {
                r: 40,
                g: 42,
                b: 54,
            },
            inline_code_tick: Color::Rgb {
                r: 68,
                g: 71,
                b: 90,
            },

            blockquote_bar: Color::Rgb {
                r: 116,
                g: 143,
                b: 196,
            },

            link: Color::Rgb {
                r: 137,
                g: 180,
                b: 250,
            },
            link_url: Color::Rgb {
                r: 108,
                g: 112,
                b: 134,
            },

            bullet: Color::Rgb {
                r: 127,
                g: 132,
                b: 156,
            },
            task_done: Color::Rgb {
                r: 166,
                g: 227,
                b: 161,
            },
            task_pending: Color::Rgb {
                r: 108,
                g: 112,
                b: 134,
            },

            rule: Color::Rgb {
                r: 68,
                g: 71,
                b: 90,
            },

            table_border: Color::Rgb {
                r: 68,
                g: 71,
                b: 90,
            },
            table_header: Color::Rgb {
                r: 137,
                g: 180,
                b: 250,
            },

            search_prompt: Color::Rgb {
                r: 249,
                g: 226,
                b: 175,
            },
            search_match_bg: Color::Rgb {
                r: 100,
                g: 80,
                b: 0,
            },
            search_current_bg: Color::Rgb {
                r: 249,
                g: 226,
                b: 175,
            },
            search_current_fg: Color::Rgb {
                r: 24,
                g: 24,
                b: 37,
            },
            search_no_match: Color::Rgb {
                r: 243,
                g: 139,
                b: 168,
            },

            overlay_bg: Color::Rgb {
                r: 36,
                g: 39,
                b: 58,
            },
            overlay_border: Color::Rgb {
                r: 91,
                g: 96,
                b: 120,
            },
            overlay_selected_bg: Color::Rgb {
                r: 68,
                g: 71,
                b: 90,
            },
            overlay_selected_fg: Color::Rgb {
                r: 205,
                g: 214,
                b: 244,
            },
            overlay_text: Color::Rgb {
                r: 186,
                g: 194,
                b: 222,
            },
            overlay_muted: Color::Rgb {
                r: 108,
                g: 112,
                b: 134,
            },

            image_fg: Color::Rgb {
                r: 166,
                g: 227,
                b: 161,
            },
            slide_indicator: Color::Rgb {
                r: 249,
                g: 226,
                b: 175,
            },
            math_fg: Color::Rgb {
                r: 242,
                g: 205,
                b: 147,
            },
            line_number: Color::Rgb {
                r: 68,
                g: 71,
                b: 90,
            },

            json_key: Color::Rgb {
                r: 137,
                g: 180,
                b: 250,
            },
            json_string: Color::Rgb {
                r: 166,
                g: 227,
                b: 161,
            },
            json_number: Color::Rgb {
                r: 250,
                g: 179,
                b: 135,
            },
            json_bool: Color::Rgb {
                r: 249,
                g: 226,
                b: 175,
            },
            json_null: Color::Rgb {
                r: 108,
                g: 112,
                b: 134,
            },
            json_bracket: Color::Rgb {
                r: 127,
                g: 132,
                b: 156,
            },
            json_path: Color::Rgb {
                r: 203,
                g: 166,
                b: 247,
            },
            json_focus_bg: Color::Rgb {
                r: 40,
                g: 42,
                b: 54,
            },

            composite_state_bg: Color::Rgb {
                r: 30,
                g: 32,
                b: 38,
            },

            key_badge_pk: Color::Rgb {
                r: 215,
                g: 195,
                b: 95,
            },
            key_badge_fk: Color::Rgb {
                r: 95,
                g: 155,
                b: 235,
            },

            member_plus: Color::Rgb {
                r: 95,
                g: 215,
                b: 135,
            },
            member_minus: Color::Rgb {
                r: 235,
                g: 95,
                b: 95,
            },
            member_hash: Color::Rgb {
                r: 215,
                g: 195,
                b: 95,
            },
            member_tilde: Color::Rgb {
                r: 95,
                g: 155,
                b: 235,
            },
        }
    }

    pub fn light() -> Self {
        Self {
            is_dark: false,

            bg: Color::Rgb {
                r: 239,
                g: 241,
                b: 245,
            },
            fg: Color::Rgb {
                r: 76,
                g: 79,
                b: 105,
            },

            border: Color::Rgb {
                r: 172,
                g: 176,
                b: 190,
            },
            title: Color::Rgb {
                r: 92,
                g: 95,
                b: 119,
            },
            position: Color::Rgb {
                r: 108,
                g: 111,
                b: 133,
            },
            help_hint: Color::Rgb {
                r: 140,
                g: 143,
                b: 161,
            },
            scrollbar_track: Color::Rgb {
                r: 204,
                g: 208,
                b: 218,
            },
            scrollbar_thumb: Color::Rgb {
                r: 140,
                g: 143,
                b: 161,
            },

            h1: Color::Rgb {
                r: 32,
                g: 32,
                b: 42,
            },
            h2: Color::Rgb {
                r: 30,
                g: 102,
                b: 245,
            },
            h3: Color::Rgb {
                r: 136,
                g: 57,
                b: 239,
            },
            h4: Color::Rgb {
                r: 64,
                g: 160,
                b: 43,
            },
            h5: Color::Rgb {
                r: 223,
                g: 142,
                b: 29,
            },
            h6: Color::Rgb {
                r: 108,
                g: 111,
                b: 133,
            },
            heading_separator: Color::Rgb {
                r: 204,
                g: 208,
                b: 218,
            },

            code_bg: Color::Rgb {
                r: 239,
                g: 241,
                b: 245,
            },
            code_border: Color::Rgb {
                r: 188,
                g: 192,
                b: 204,
            },
            code_label: Color::Rgb {
                r: 124,
                g: 127,
                b: 147,
            },
            diagram_error_fg: Color::Rgb {
                r: 180,
                g: 34,
                b: 36,
            },
            syntect_theme: "InspiredGitHub",

            inline_code_fg: Color::Rgb {
                r: 179,
                g: 82,
                b: 2,
            },
            inline_code_bg: Color::Rgb {
                r: 230,
                g: 233,
                b: 239,
            },
            inline_code_tick: Color::Rgb {
                r: 172,
                g: 176,
                b: 190,
            },

            blockquote_bar: Color::Rgb {
                r: 30,
                g: 102,
                b: 245,
            },

            link: Color::Rgb {
                r: 30,
                g: 102,
                b: 245,
            },
            link_url: Color::Rgb {
                r: 140,
                g: 143,
                b: 161,
            },

            bullet: Color::Rgb {
                r: 108,
                g: 111,
                b: 133,
            },
            task_done: Color::Rgb {
                r: 64,
                g: 160,
                b: 43,
            },
            task_pending: Color::Rgb {
                r: 140,
                g: 143,
                b: 161,
            },

            rule: Color::Rgb {
                r: 188,
                g: 192,
                b: 204,
            },

            table_border: Color::Rgb {
                r: 188,
                g: 192,
                b: 204,
            },
            table_header: Color::Rgb {
                r: 30,
                g: 102,
                b: 245,
            },

            search_prompt: Color::Rgb {
                r: 223,
                g: 142,
                b: 29,
            },
            search_match_bg: Color::Rgb {
                r: 255,
                g: 235,
                b: 160,
            },
            search_current_bg: Color::Rgb {
                r: 253,
                g: 205,
                b: 54,
            },
            search_current_fg: Color::Rgb {
                r: 32,
                g: 32,
                b: 42,
            },
            search_no_match: Color::Rgb {
                r: 210,
                g: 15,
                b: 57,
            },

            overlay_bg: Color::Rgb {
                r: 230,
                g: 233,
                b: 239,
            },
            overlay_border: Color::Rgb {
                r: 172,
                g: 176,
                b: 190,
            },
            overlay_selected_bg: Color::Rgb {
                r: 188,
                g: 192,
                b: 204,
            },
            overlay_selected_fg: Color::Rgb {
                r: 76,
                g: 79,
                b: 105,
            },
            overlay_text: Color::Rgb {
                r: 76,
                g: 79,
                b: 105,
            },
            overlay_muted: Color::Rgb {
                r: 140,
                g: 143,
                b: 161,
            },

            image_fg: Color::Rgb {
                r: 64,
                g: 160,
                b: 43,
            },
            slide_indicator: Color::Rgb {
                r: 223,
                g: 142,
                b: 29,
            },
            math_fg: Color::Rgb {
                r: 179,
                g: 82,
                b: 2,
            },
            line_number: Color::Rgb {
                r: 172,
                g: 176,
                b: 190,
            },

            json_key: Color::Rgb {
                r: 30,
                g: 102,
                b: 245,
            },
            json_string: Color::Rgb {
                r: 64,
                g: 160,
                b: 43,
            },
            json_number: Color::Rgb {
                r: 254,
                g: 100,
                b: 11,
            },
            json_bool: Color::Rgb {
                r: 223,
                g: 142,
                b: 29,
            },
            json_null: Color::Rgb {
                r: 140,
                g: 143,
                b: 161,
            },
            json_bracket: Color::Rgb {
                r: 108,
                g: 111,
                b: 133,
            },
            json_path: Color::Rgb {
                r: 136,
                g: 57,
                b: 239,
            },
            json_focus_bg: Color::Rgb {
                r: 220,
                g: 224,
                b: 232,
            },

            composite_state_bg: Color::Rgb {
                r: 238,
                g: 240,
                b: 244,
            },

            key_badge_pk: Color::Rgb {
                r: 140,
                g: 104,
                b: 0,
            },
            key_badge_fk: Color::Rgb {
                r: 0,
                g: 118,
                b: 168,
            },

            member_plus: Color::Rgb {
                r: 0,
                g: 115,
                b: 73,
            },
            member_minus: Color::Rgb {
                r: 203,
                g: 36,
                b: 49,
            },
            member_hash: Color::Rgb {
                r: 140,
                g: 104,
                b: 0,
            },
            member_tilde: Color::Rgb {
                r: 0,
                g: 118,
                b: 168,
            },
        }
    }

    pub fn toggle(&self) -> Self {
        if self.is_dark {
            Self::light()
        } else {
            Self::dark()
        }
    }

    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        if self.is_dark { "dark" } else { "light" }
    }
}
