use widestring::{u16str, U16CString};
use windows::{
    core::PCWSTR,
    Foundation::Numerics::Matrix3x2,
    Win32::{
        Foundation::HWND,
        Graphics::{
            Direct2D::{
                Common::{
                    D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_POINT_2F,
                    D2D_RECT_F, D2D_SIZE_U,
                },
                D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget, D2D1_BRUSH_PROPERTIES,
                D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
                D2D1_FEATURE_LEVEL_DEFAULT, D2D1_HWND_RENDER_TARGET_PROPERTIES,
                D2D1_PRESENT_OPTIONS_IMMEDIATELY, D2D1_RENDER_TARGET_PROPERTIES,
                D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE,
            },
            DirectWrite::{
                DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
                DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_HIT_TEST_METRICS, DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE,
                DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP,
            },
            Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
        },
    },
};
use winit::{platform::windows::WindowExtWindows, window::Window};

use crate::{
    renderer::{Color, TextEffect, TextEffectKind},
    theme::TEXT_COLOR,
    view::View,
};

pub struct GraphicsContext {
    window_size: (f32, f32),
    render_target: ID2D1HwndRenderTarget,
    dwrite_factory: IDWriteFactory,
    text_format: IDWriteTextFormat,
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
                    PCWSTR(U16CString::from_str("Consolas").unwrap().into_raw()),
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    18.0,
                    PCWSTR(U16CString::from_str("en-us").unwrap().into_raw()),
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
                .CreateTextLayout(u16str!(" ").as_slice(), &text_format, 0.0, 0.0)
                .unwrap()
        };

        let mut metrics = DWRITE_HIT_TEST_METRICS::default();
        let mut _dummy: (f32, f32) = (0.0, 0.0);
        unsafe {
            text_layout
                .HitTestTextPosition(0, false, &mut _dummy.0, &mut _dummy.1, &mut metrics)
                .unwrap();
        }

        let font_size = (metrics.width, metrics.height);

        Self {
            window_size,
            dwrite_factory,
            render_target,
            text_format,
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
                    top: row_offset - 0.5,
                    right: col_offset + self.font_size.0 * size.0 as f32 + 0.5,
                    bottom: row_offset + self.font_size.1 * size.1 as f32 + 0.5,
                },
                &brush,
            );
        }
    }

    pub fn fill_cell_slim_line(&self, row: usize, col: usize, color: Color) {
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
                    top: row_offset - 0.5,
                    right: col_offset + self.font_size.0 * 0.1 + 0.5,
                    bottom: row_offset + self.font_size.1 + 0.5,
                },
                &brush,
            );
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
                    bottom: row_offset + self.font_size.1 as f32 + 0.5,
                },
                &brush,
            );
        }
    }

    pub fn get_text_bounding_box(&self, text: &[u8]) -> (f64, f64) {
        let text_layout = unsafe {
            self.dwrite_factory
                .CreateTextLayout(
                    U16CString::from_str(std::str::from_utf8(text).unwrap())
                        .unwrap()
                        .as_slice(),
                    &self.text_format,
                    self.window_size.0,
                    self.window_size.1,
                )
                .unwrap()
        };

        let mut text_metrics = DWRITE_TEXT_METRICS::default();
        unsafe {
            text_layout.GetMetrics(&mut text_metrics as *mut _).unwrap();
        }

        (text_metrics.width as f64, text_metrics.height as f64)
    }

    pub fn get_wrapping_text_bounding_box(&self, text: &[u8]) -> (f64, f64) {
        let text_layout = unsafe {
            self.dwrite_factory
                .CreateTextLayout(
                    U16CString::from_str(std::str::from_utf8(text).unwrap())
                        .unwrap()
                        .as_slice(),
                    &self.text_format,
                    self.window_size.0,
                    self.window_size.1,
                )
                .unwrap()
        };

        let mut text_metrics = DWRITE_TEXT_METRICS::default();
        unsafe {
            text_layout.GetMetrics(&mut text_metrics as *mut _).unwrap();
        }

        (text_metrics.width as f64, text_metrics.height as f64)
    }

    pub fn draw_text_with_col_offset(
        &self,
        row: usize,
        col: usize,
        text: &[u8],
        effects: &[TextEffect],
        col_offset: usize,
    ) {
        let text_layout = unsafe {
            self.dwrite_factory
                .CreateTextLayout(
                    U16CString::from_str(std::str::from_utf8(text).unwrap())
                        .unwrap()
                        .as_slice(),
                    &self.text_format,
                    self.window_size.0,
                    self.window_size.1,
                )
                .unwrap()
        };

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
                        r: TEXT_COLOR.r,
                        g: TEXT_COLOR.g,
                        b: TEXT_COLOR.b,
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

    pub fn draw_text(&self, row: usize, col: usize, text: &[u8], effects: &[TextEffect]) {
        self.draw_text_with_col_offset(row, col, text, effects, 0)
    }

    pub fn draw_text_fit_view(&self, view: &View, text: &[u8], effects: &[TextEffect]) {
        self.draw_text_with_col_offset(0, 0, text, effects, view.col_offset)
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
}

const DEFAULT_BRUSH_PROPERTIES: D2D1_BRUSH_PROPERTIES = D2D1_BRUSH_PROPERTIES {
    opacity: 1.0,
    transform: Matrix3x2::identity(),
};
