use crate::renderer::Color;

// Palette: https://github.com/sainnhe/everforest
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
    pub keyword_color: Color,
    pub type_color: Color,
    pub string_color: Color,
    pub comment_color: Color,
    pub function_color: Color,
    pub constant_color: Color,
}

pub const EVERFOREST_DARK: Theme = Theme {
    background_color: Color::from_rgb(39, 46, 51),
    foreground_color: Color::from_rgb(211, 198, 170),
    selection_background_color: Color::from_rgb(65, 75, 80),
    cursor_color: Color::from_rgb(211, 198, 170),
    diagnostic_color: Color::from_rgb(230, 126, 128),
    numbers_color: Color::from_rgb(133, 146, 137),
    search_foreground_color: Color::from_rgb(39, 46, 51),
    active_search_foreground_color: Color::from_rgb(39, 46, 51),
    search_background_color: Color::from_rgb(167, 192, 128),
    active_search_background_color: Color::from_rgb(230, 127, 128),
    active_parameter_color: Color::from_rgb(167, 192, 128),
    status_line_background_color: Color::from_rgb(18, 25, 37),
    keyword_color: Color::from_rgb(230, 152, 117),
    type_color: Color::from_rgb(219, 188, 127),
    string_color: Color::from_rgb(131, 192, 146),
    comment_color: Color::from_rgb(133, 146, 137),
    function_color: Color::from_rgb(127, 187, 179),
    constant_color: Color::from_rgb(214, 153, 182),
};

pub const EVERFOREST_LIGHT: Theme = Theme {
    background_color: Color::from_rgb(253, 246, 227),
    foreground_color: Color::from_rgb(92, 106, 114),
    selection_background_color: Color::from_rgb(230, 226, 204),
    cursor_color: Color::from_rgb(92, 106, 114),
    diagnostic_color: Color::from_rgb(248, 85, 82),
    numbers_color: Color::from_rgb(147, 159, 145),
    search_foreground_color: Color::from_rgb(253, 246, 227),
    active_search_foreground_color: Color::from_rgb(253, 246, 227),
    search_background_color: Color::from_rgb(141, 161, 1),
    active_search_background_color: Color::from_rgb(248, 85, 82),
    active_parameter_color: Color::from_rgb(141, 161, 1),
    status_line_background_color: Color::from_rgb(253, 253, 252),
    keyword_color: Color::from_rgb(245, 125, 38),
    type_color: Color::from_rgb(223, 160, 0),
    string_color: Color::from_rgb(53, 167, 124),
    comment_color: Color::from_rgb(147, 159, 145),
    function_color: Color::from_rgb(58, 148, 197),
    constant_color: Color::from_rgb(223, 105, 186),
};

pub const THEMES: [Theme; 2] = [EVERFOREST_DARK, EVERFOREST_LIGHT];
