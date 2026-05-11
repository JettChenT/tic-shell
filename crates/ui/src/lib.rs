use gpui::{Hsla, hsla};

pub const DEFAULT_FONT_FAMILY: &str = "DejaVu Sans";

#[derive(Debug, Clone)]
pub struct Theme {
    pub bg: Hsla,
    pub bg_muted: Hsla,
    pub bg_hover: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub accent: Hsla,
    pub warning: Hsla,
    pub error: Hsla,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: hsla(222.0 / 360.0, 0.16, 0.15, 1.0),
            bg_muted: hsla(223.0 / 360.0, 0.14, 0.20, 1.0),
            bg_hover: hsla(224.0 / 360.0, 0.14, 0.26, 1.0),
            border: hsla(224.0 / 360.0, 0.13, 0.40, 1.0),
            text: hsla(229.0 / 360.0, 0.56, 0.88, 1.0),
            text_muted: hsla(228.0 / 360.0, 0.16, 0.62, 1.0),
            accent: hsla(172.0 / 360.0, 0.46, 0.72, 1.0),
            warning: hsla(40.0 / 360.0, 0.78, 0.70, 1.0),
            error: hsla(351.0 / 360.0, 0.74, 0.74, 1.0),
        }
    }
}

pub mod sizes {
    pub const COLLAPSED_WIDTH: f32 = 44.0;
    pub const WORKSPACE_WIDTH: f32 = 250.0;
    pub const AGENT_WIDTH: f32 = 360.0;
    pub const DIVIDER_WIDTH: f32 = 1.0;
}
