use crate::renderer::Color;

// Palette inspiration: https://github.com/sainnhe/everforest
#[derive(Clone, Copy, PartialEq)]
pub struct Palette {
    pub bg0: Color,
    pub bg1: Color,
    pub bg2: Color,
    pub bg_dim: Color,
    pub fg0: Color,
    pub red: Color,
    pub orange: Color,
    pub yellow: Color,
    pub green: Color,
    pub aqua: Color,
    pub blue: Color,
    pub pink: Color,
}

const EVERFOREST_DARK_PALETTE: Palette = Palette {
    bg0: Color::from_rgb(39, 46, 51),
    bg1: Color::from_rgb(65, 75, 80),
    bg2: Color::from_rgb(133, 146, 137),
    bg_dim: Color::from_rgb(18, 25, 37),
    fg0: Color::from_rgb(211, 198, 170),
    red: Color::from_rgb(230, 126, 128),
    orange: Color::from_rgb(230, 152, 117),
    yellow: Color::from_rgb(219, 188, 127),
    green: Color::from_rgb(167, 192, 128),
    aqua: Color::from_rgb(131, 192, 146),
    blue: Color::from_rgb(127, 187, 179),
    pink: Color::from_rgb(214, 153, 182),
};

const EVERFOREST_LIGHT_PALETTE: Palette = Palette {
    bg0: Color::from_rgb(253, 246, 227),
    bg1: Color::from_rgb(230, 226, 204),
    bg2: Color::from_rgb(147, 159, 145),
    bg_dim: Color::from_rgb(253, 253, 252),
    fg0: Color::from_rgb(92, 106, 114),
    red: Color::from_rgb(248, 85, 82),
    orange: Color::from_rgb(245, 125, 38),
    yellow: Color::from_rgb(223, 160, 0),
    green: Color::from_rgb(141, 161, 1),
    aqua: Color::from_rgb(53, 167, 124),
    blue: Color::from_rgb(58, 148, 197),
    pink: Color::from_rgb(223, 105, 186),
};

#[derive(Clone, Copy, PartialEq)]
pub struct Theme {
    pub background_color: Color,
    pub foreground_color: Color,
    pub selection_background_color: Color,
    pub cursor_color: Color,
    pub diagnostic_color: Color,
    pub numbers_color: Color,
    pub search_foreground_color: Color,
    pub active_search_foreground_color: Color,
    pub search_background_color: Color,
    pub active_search_background_color: Color,
    pub active_parameter_color: Color,
    pub status_line_background_color: Color,
    pub palette: Palette,
}

impl Theme {
    const fn new(palette: Palette) -> Self {
        Self {
            background_color: palette.bg0,
            foreground_color: palette.fg0,
            selection_background_color: palette.bg1,
            cursor_color: palette.fg0,
            diagnostic_color: palette.red,
            numbers_color: palette.bg2,
            search_foreground_color: palette.bg0,
            active_search_foreground_color: palette.bg0,
            search_background_color: palette.green,
            active_search_background_color: palette.red,
            active_parameter_color: palette.green,
            status_line_background_color: palette.bg_dim,
            palette,
        }
    }
}

pub const EVERFOREST_DARK: Theme = Theme::new(EVERFOREST_DARK_PALETTE);
pub const EVERFOREST_LIGHT: Theme = Theme::new(EVERFOREST_LIGHT_PALETTE);

pub const THEMES: [Theme; 2] = [EVERFOREST_DARK, EVERFOREST_LIGHT];
