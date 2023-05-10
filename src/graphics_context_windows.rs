use windows::{
    core::ComInterface,
    w,
    Foundation::Numerics::Matrix3x2,
    Win32::{
        Foundation::HWND,
        Graphics::{
            Direct2D::{
                Common::{
                    D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_POINT_2F,
                    D2D_RECT_F, D2D_SIZE_U,
                },
                D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget,
                D2D1_ANTIALIAS_MODE_ALIASED, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                D2D1_BRUSH_PROPERTIES, D2D1_DRAW_TEXT_OPTIONS_NONE,
                D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
                D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_IMMEDIATELY,
                D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
                D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
            },
            DirectWrite::{
                DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout1,
                DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_WEIGHT_NORMAL, DWRITE_HIT_TEST_METRICS, DWRITE_TEXT_METRICS,
                DWRITE_TEXT_RANGE, DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP,
            },
            Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
        },
    },
};
use winit::{platform::windows::WindowExtWindows, window::Window};

use crate::{
    renderer::{Color, TextEffect, TextEffectKind},
    theme::Theme,
    view::View,
};

pub struct GraphicsContext {
    window_size: (f32, f32),
    render_target: ID2D1HwndRenderTarget,
    dwrite_factory: IDWriteFactory,
    text_format: IDWriteTextFormat,
    character_spacing: f32,
    pub font_size: (f32, f32),
}

impl GraphicsContext {
    pub fn new(window: &Window) -> Self {
        let window_size = (
            window.inner_size().width as f32 / window.scale_factor() as f32,
            window.inner_size().height as f32 / window.scale_factor() as f32,
        );

        let d2d1_factory: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).unwrap() };

        let render_target = unsafe {
            d2d1_factory
                .CreateHwndRenderTarget(
                    &D2D1_RENDER_TARGET_PROPERTIES {
                        r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_R8G8B8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_IGNORE,
                        },
                        dpiX: 0.0,
                        dpiY: 0.0,
                        usage: D2D1_RENDER_TARGET_USAGE_NONE,
                        minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
                    },
                    &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                        hwnd: HWND(window.hwnd()),
                        pixelSize: D2D_SIZE_U {
                            width: window.inner_size().width,
                            height: window.inner_size().height,
                        },
                        presentOptions: D2D1_PRESENT_OPTIONS_IMMEDIATELY,
                    },
                )
                .unwrap()
        };

        let dwrite_factory: IDWriteFactory =
            unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).unwrap() };

        let text_format = unsafe {
            dwrite_factory
                .CreateTextFormat(
                    w!("Consolas"),
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    18.0,
                    w!("en-us"),
                )
                .unwrap()
        };
        unsafe {
            text_format
                .SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)
                .unwrap();
        }

        let text_layout = unsafe {
            dwrite_factory
                .CreateTextLayout(&[b' ' as u16], &text_format, 0.0, 0.0)
                .unwrap()
        };

        let mut metrics = DWRITE_HIT_TEST_METRICS::default();
        let mut _dummy: (f32, f32) = (0.0, 0.0);
        unsafe {
            text_layout
                .HitTestTextPosition(0, false, &mut _dummy.0, &mut _dummy.1, &mut metrics)
                .unwrap();
        }

        let character_spacing = (metrics.width.ceil() - metrics.width) / 2.0;
        let font_size = (metrics.width.ceil(), metrics.height);

        Self {
            window_size,
            dwrite_factory,
            render_target,
            text_format,
            character_spacing,
            font_size,
        }
    }

    pub fn begin_draw(&self) {
        unsafe {
            self.render_target.BeginDraw();
        }
    }

    pub fn end_draw(&self) {
        unsafe {
            self.render_target.EndDraw(None, None).unwrap();
        }
    }

    pub fn clear(&self, color: Color) {
        unsafe {
            self.render_target.Clear(Some(&D2D1_COLOR_F {
                r: color.r,
                g: color.g,
                b: color.b,
                a: 1.0,
            }));
        }
    }

    pub fn fill_cells(&self, row: usize, col: usize, size: (usize, usize), color: Color) {
        let (row_offset, col_offset) =
            (row as f32 * self.font_size.1, col as f32 * self.font_size.0);

        unsafe {
            self.render_target
                .SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);
            let brush = self
                .render_target
                .CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: color.r,
                        g: color.g,
                        b: color.b,
                        a: 1.0,
                    },
                    Some(&DEFAULT_BRUSH_PROPERTIES),
                )
                .unwrap();

            self.render_target.FillRectangle(
                &D2D_RECT_F {
                    left: col_offset,
                    top: row_offset - 0.5,
                    right: col_offset + self.font_size.0 * size.0 as f32,
                    bottom: row_offset + self.font_size.1 * size.1 as f32 + 0.5,
                },
                &brush,
            );
            self.render_target
                .SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        }
    }

    pub fn fill_cell_slim_line(&self, row: usize, col: usize, color: Color) {
        let (row_offset, col_offset) =
            (row as f32 * self.font_size.1, col as f32 * self.font_size.0);

        unsafe {
            self.render_target
                .SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);
            let brush = self
                .render_target
                .CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: color.r,
                        g: color.g,
                        b: color.b,
                        a: 1.0,
                    },
                    Some(&DEFAULT_BRUSH_PROPERTIES),
                )
                .unwrap();
            self.render_target.FillRectangle(
                &D2D_RECT_F {
                    left: col_offset,
                    top: row_offset - 0.5,
                    right: col_offset + self.font_size.0 * 0.15,
                    bottom: row_offset + self.font_size.1 + 0.5,
                },
                &brush,
            );
            self.render_target
                .SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        }
    }

    pub fn underline_cells(&self, row: usize, col: usize, count: usize, color: Color) {
        let (row_offset, col_offset) =
            (row as f32 * self.font_size.1, col as f32 * self.font_size.0);

        unsafe {
            let brush = self
                .render_target
                .CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: color.r,
                        g: color.g,
                        b: color.b,
                        a: 1.0,
                    },
                    Some(&DEFAULT_BRUSH_PROPERTIES),
                )
                .unwrap();

            self.render_target.FillRectangle(
                &D2D_RECT_F {
                    left: col_offset - 0.5,
                    top: row_offset + self.font_size.1 * 0.98 - 0.5,
                    right: col_offset + self.font_size.0 * count as f32 + 0.5,
                    bottom: row_offset + self.font_size.1 + 0.5,
                },
                &brush,
            );
        }
    }

    pub fn get_text_bounding_box(&self, text: &[u8]) -> (f64, f64) {
        let mut wide_text = vec![];
        for c in text {
            wide_text.push(*c as u16);
        }

        let text_layout = unsafe {
            self.dwrite_factory
                .CreateTextLayout(
                    &wide_text,
                    &self.text_format,
                    self.window_size.0,
                    self.window_size.1,
                )
                .unwrap()
        };

        let mut text_metrics = DWRITE_TEXT_METRICS::default();
        unsafe {
            text_layout
                .cast::<IDWriteTextLayout1>()
                .unwrap()
                .SetCharacterSpacing(
                    self.character_spacing,
                    self.character_spacing,
                    self.character_spacing,
                    DWRITE_TEXT_RANGE {
                        startPosition: 0,
                        length: wide_text.len() as u32,
                    },
                )
                .unwrap();

            text_layout.GetMetrics(&mut text_metrics as *mut _).unwrap();
        }

        (text_metrics.width as f64, text_metrics.height as f64)
    }

    fn draw_text_with_offset(
        &self,
        x: f32,
        y: f32,
        text: &[u8],
        effects: &[TextEffect],
        theme: &Theme,
    ) {
        let mut wide_text = vec![];
        for c in text {
            wide_text.push(*c as u16);
        }

        let text_layout = unsafe {
            self.dwrite_factory
                .CreateTextLayout(
                    &wide_text,
                    &self.text_format,
                    self.window_size.0,
                    self.window_size.1,
                )
                .unwrap()
        };

        unsafe {
            text_layout
                .cast::<IDWriteTextLayout1>()
                .unwrap()
                .SetCharacterSpacing(
                    self.character_spacing,
                    self.character_spacing,
                    self.character_spacing,
                    DWRITE_TEXT_RANGE {
                        startPosition: 0,
                        length: wide_text.len() as u32,
                    },
                )
                .unwrap();
        }

        for effect in effects {
            match &effect.kind {
                TextEffectKind::ForegroundColor(color) => unsafe {
                    let brush = self
                        .render_target
                        .CreateSolidColorBrush(
                            &D2D1_COLOR_F {
                                r: color.r,
                                g: color.g,
                                b: color.b,
                                a: 1.0,
                            },
                            Some(&DEFAULT_BRUSH_PROPERTIES),
                        )
                        .unwrap();

                    text_layout
                        .SetDrawingEffect(
                            &brush,
                            DWRITE_TEXT_RANGE {
                                startPosition: effect.start as u32,
                                length: effect.length as u32,
                            },
                        )
                        .unwrap();
                },
            }
        }

        unsafe {
            let brush = self
                .render_target
                .CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: theme.foreground_color.r,
                        g: theme.foreground_color.g,
                        b: theme.foreground_color.b,
                        a: 1.0,
                    },
                    Some(&DEFAULT_BRUSH_PROPERTIES),
                )
                .unwrap();

            self.render_target.DrawTextLayout(
                D2D_POINT_2F { x, y },
                &text_layout,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
        }
    }

    pub fn draw_text_with_col_offset(
        &self,
        row: usize,
        col: usize,
        text: &[u8],
        effects: &[TextEffect],
        theme: &Theme,
        col_offset: usize,
    ) {
        let mut wide_text = vec![];
        for c in text {
            wide_text.push(*c as u16);
        }

        let text_layout = unsafe {
            self.dwrite_factory
                .CreateTextLayout(
                    &wide_text,
                    &self.text_format,
                    self.window_size.0,
                    self.window_size.1,
                )
                .unwrap()
        };

        unsafe {
            text_layout
                .cast::<IDWriteTextLayout1>()
                .unwrap()
                .SetCharacterSpacing(
                    self.character_spacing,
                    self.character_spacing,
                    self.character_spacing,
                    DWRITE_TEXT_RANGE {
                        startPosition: 0,
                        length: wide_text.len() as u32,
                    },
                )
                .unwrap();
        }

        for effect in effects {
            match &effect.kind {
                TextEffectKind::ForegroundColor(color) => unsafe {
                    let brush = self
                        .render_target
                        .CreateSolidColorBrush(
                            &D2D1_COLOR_F {
                                r: color.r,
                                g: color.g,
                                b: color.b,
                                a: 1.0,
                            },
                            Some(&DEFAULT_BRUSH_PROPERTIES),
                        )
                        .unwrap();

                    text_layout
                        .SetDrawingEffect(
                            &brush,
                            DWRITE_TEXT_RANGE {
                                startPosition: effect.start as u32,
                                length: effect.length as u32,
                            },
                        )
                        .unwrap();
                },
            }
        }

        unsafe {
            let brush = self
                .render_target
                .CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: theme.foreground_color.r,
                        g: theme.foreground_color.g,
                        b: theme.foreground_color.b,
                        a: 1.0,
                    },
                    Some(&DEFAULT_BRUSH_PROPERTIES),
                )
                .unwrap();

            self.render_target.DrawTextLayout(
                D2D_POINT_2F {
                    x: -self.font_size.0 * col_offset as f32 + self.font_size.0 * col as f32,
                    y: self.font_size.1 * row as f32,
                },
                &text_layout,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
        }
    }

    pub fn draw_text(
        &self,
        row: usize,
        col: usize,
        text: &[u8],
        effects: &[TextEffect],
        theme: &Theme,
    ) {
        self.draw_text_with_col_offset(row, col, text, effects, theme, 0)
    }

    pub fn draw_text_fit_view(
        &self,
        view: &View,
        text: &[u8],
        effects: &[TextEffect],
        theme: &Theme,
    ) {
        self.draw_text_with_col_offset(0, 0, text, effects, theme, view.col_offset)
    }

    pub fn set_word_wrapping(&self, wrap: bool) {
        unsafe {
            if wrap {
                self.text_format
                    .SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)
                    .unwrap();
            } else {
                self.text_format
                    .SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)
                    .unwrap();
            }
        }
    }

    pub fn draw_popup(
        &self,
        row: usize,
        col: usize,
        grow_up: bool,
        text: &[u8],
        outer_color: Color,
        inner_color: Color,
        effects: Option<&[TextEffect]>,
        theme: &Theme,
    ) {
        self.set_word_wrapping(true);

        let (width, height) = self.get_text_bounding_box(text);

        let (mut row_offset, col_offset) =
            (row as f32 * self.font_size.1, col as f32 * self.font_size.0);

        if grow_up {
            row_offset -= height as f32 + self.font_size.1 + self.font_size.1;
        }

        let (width, height) = (
            (width / self.font_size.0 as f64).round() as usize,
            (height / self.font_size.1 as f64).round() as usize,
        );

        unsafe {
            let outer_brush = self
                .render_target
                .CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: outer_color.r,
                        g: outer_color.g,
                        b: outer_color.b,
                        a: 1.0,
                    },
                    Some(&DEFAULT_BRUSH_PROPERTIES),
                )
                .unwrap();

            self.render_target.FillRectangle(
                &D2D_RECT_F {
                    left: col_offset - 0.5,
                    top: row_offset - 0.5,
                    right: col_offset + self.font_size.0 * width as f32 + self.font_size.1 + 0.5,
                    bottom: row_offset + self.font_size.1 * height as f32 + self.font_size.1 + 0.5,
                },
                &outer_brush,
            );

            let inner_brush = self
                .render_target
                .CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: inner_color.r,
                        g: inner_color.g,
                        b: inner_color.b,
                        a: 1.0,
                    },
                    Some(&DEFAULT_BRUSH_PROPERTIES),
                )
                .unwrap();

            self.render_target.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: col_offset - 0.5 + self.font_size.1 * 0.25,
                        top: row_offset - 0.5 + self.font_size.1 * 0.25,
                        right: col_offset
                            + self.font_size.0 * width as f32
                            + self.font_size.1 * 0.75
                            + 0.5,
                        bottom: row_offset
                            + self.font_size.1 * height as f32
                            + self.font_size.1 * 0.75
                            + 0.5,
                    },
                    radiusX: 2.0,
                    radiusY: 2.0,
                },
                &inner_brush,
            );
        }

        self.draw_text_with_offset(
            col_offset + self.font_size.1 * 0.5,
            row_offset + self.font_size.1 * 0.5,
            text,
            effects.unwrap_or(&[]),
            theme,
        );

        self.set_word_wrapping(false);
    }
}

const DEFAULT_BRUSH_PROPERTIES: D2D1_BRUSH_PROPERTIES = D2D1_BRUSH_PROPERTIES {
    opacity: 1.0,
    transform: Matrix3x2::identity(),
};
