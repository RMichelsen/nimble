use crate::renderer::Color;

// Palette: https://github.com/catppuccin/catppuccin

#[derive(Clone, Copy, PartialEq)]
pub struct Theme {
    pub background_color: Color,
    pub foreground_color: Color,
    pub selection_background_color: Color,
    pub cursor_color: Color,
    pub diagnostic_color: Color,
    pub search_foreground_color: Color,
    pub active_search_foreground_color: Color,

    // "keyword",
    // "type.builtin",
    // "type",
    // "string_literal",
    // "comment",
    pub tree_sitter_colors: [Color; 5],
}

pub const EVERFOREST: Theme = Theme {
    background_color: Color::from_rgb(39, 46, 51),
    foreground_color: Color::from_rgb(211, 198, 170),
    selection_background_color: Color::from_rgb(82, 87, 91),
    cursor_color: Color::from_rgb(211, 198, 170),
    diagnostic_color: Color::from_rgb(254, 128, 25),
    search_foreground_color: Color::from_rgb(255, 255, 255),
    active_search_foreground_color: Color::from_rgb(255, 255, 255),

    tree_sitter_colors: [
        Color::from_rgb(230, 126, 128),
        Color::from_rgb(219, 188, 127),
        Color::from_rgb(219, 188, 127),
        Color::from_rgb(131, 192, 146),
        Color::from_rgb(167, 192, 128),
    ],
};

pub const THEMES: [Theme; 1] = [EVERFOREST];
