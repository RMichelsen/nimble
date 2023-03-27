use windows::{
    Foundation::Numerics::Matrix3x2,
    Win32::Graphics::Direct2D::{Common::D2D1_COLOR_F, D2D1_BRUSH_PROPERTIES},
};

pub const BACKGROUND_COLOR: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 0.02,
    g: 0.14,
    b: 0.16,
    a: 1.0,
};

pub const TEXT_COLOR: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 0.86,
    g: 0.77,
    b: 0.64,
    a: 1.0,
};

pub const KEYWORD_COLOR: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 1.0,
    g: 1.0,
    b: 1.0,
    a: 1.0,
};

pub const HIGHLIGHT_COLOR: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 0.25,
    g: 0.25,
    b: 0.70,
    a: 1.0,
};

pub const COMMENT_COLOR: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 0.55,
    g: 0.87,
    b: 0.58,
    a: 1.0,
};

pub const DEFAULT_BRUSH_PROPERTIES: D2D1_BRUSH_PROPERTIES = D2D1_BRUSH_PROPERTIES {
    opacity: 1.0,
    transform: Matrix3x2::identity(),
};
