use windows::{
    core::ComInterface,
    w,
    Foundation::Numerics::Matrix3x2,
    Win32::{
        Foundation::HWND,
        Globalization::{MultiByteToWideChar, CP_UTF8, MULTI_BYTE_TO_WIDE_CHAR_FLAGS},
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
                D2D1_RENDER_TARGET_USAGE_NONE,
            },
            DirectWrite::{
                DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout1,
                DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_WEIGHT_NORMAL, DWRITE_HIT_TEST_METRICS, DWRITE_TEXT_ALIGNMENT_TRAILING,
                DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_WORD_WRAPPING_NO_WRAP,
                DWRITE_WORD_WRAPPING_WRAP,
            },
            Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
        },
    },
};
use winit::{platform::windows::WindowExtWindows, window::Window};

use crate::{
    renderer::{Color, RenderLayout, TextEffect, TextEffectKind},
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

    pub fn ensure_size(&mut self, window: &Window) {
        unsafe {
            self.render_target
                .Resize(&D2D_SIZE_U {
                    width: window.inner_size().width,
                    height: window.inner_size().height,
                })
                .unwrap();
        }

        self.window_size = (
            window.inner_size().width as f32 / window.scale_factor() as f32,
            window.inner_size().height as f32 / window.scale_factor() as f32,
        );
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

    pub fn fill_cells(
        &self,
        row: usize,
        col: usize,
        layout: &RenderLayout,
        size: (usize, usize),
        color: Color,
    ) {
        let (row_offset, col_offset) = (
            (row + layout.row_offset) as f32 * self.font_size.1,
            (col + layout.col_offset) as f32 * self.font_size.0,
        );

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

    pub fn fill_cell_slim_line(&self, row: usize, col: usize, layout: &RenderLayout, color: Color) {
        let (row_offset, col_offset) = (
            (row + layout.row_offset) as f32 * self.font_size.1,
            (col + layout.col_offset) as f32 * self.font_size.0,
        );
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

    pub fn underline_cells(
        &self,
        row: usize,
        col: usize,
        layout: &RenderLayout,
        count: usize,
        color: Color,
    ) {
        let (row_offset, col_offset) = (
            (row + layout.row_offset) as f32 * self.font_size.1,
            (col + layout.col_offset) as f32 * self.font_size.0,
        );

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

    fn get_text_width_height(
        &self,
        x: f32,
        y: f32,
        layout: &RenderLayout,
        text: &[u8],
    ) -> (f64, f64) {
        let mut wide_text = vec![];
        let wide_text_len =
            unsafe { MultiByteToWideChar(CP_UTF8, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), text, None) };
        if wide_text_len > 0 {
            wide_text.resize(wide_text_len as usize + 1, 0);
            unsafe {
                MultiByteToWideChar(
                    CP_UTF8,
                    MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0),
                    text,
                    Some(wide_text.as_mut_slice()),
                )
            };
        } else {
            for c in text {
                wide_text.push(*c as u16);
            }
        }

        let text_layout = unsafe {
            self.dwrite_factory
                .CreateTextLayout(
                    &wide_text,
                    &self.text_format,
                    self.font_size.0 * layout.num_cols as f32,
                    self.font_size.1 * layout.num_rows as f32,
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
        layout: &RenderLayout,
        text: &[u8],
        effects: &[TextEffect],
        theme: &Theme,
    ) {
        let mut wide_text = vec![];
        let wide_text_len =
            unsafe { MultiByteToWideChar(CP_UTF8, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), text, None) };
        if wide_text_len > 0 {
            wide_text.resize(wide_text_len as usize + 1, 0);
            unsafe {
                MultiByteToWideChar(
                    CP_UTF8,
                    MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0),
                    text,
                    Some(wide_text.as_mut_slice()),
                )
            };
        } else {
            for c in text {
                wide_text.push(*c as u16);
            }
        }

        let text_layout = unsafe {
            self.dwrite_factory
                .CreateTextLayout(
                    &wide_text,
                    &self.text_format,
                    self.font_size.0 * layout.num_cols as f32,
                    self.font_size.1 * layout.num_rows as f32,
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
        layout: &RenderLayout,
        text: &[u8],
        effects: &[TextEffect],
        theme: &Theme,
        col_offset: usize,
        align_right: bool,
    ) {
        // Col offset text will not use conversion because only ASCII is allowed
        let mut wide_text = vec![];
        for c in text {
            wide_text.push(*c as u16);
        }

        let text_layout = unsafe {
            self.dwrite_factory
                .CreateTextLayout(
                    &wide_text,
                    &self.text_format,
                    self.font_size.0 * layout.num_cols as f32,
                    self.font_size.1 * layout.num_rows as f32,
                )
                .unwrap()
        };

        unsafe {
            if align_right {
                text_layout
                    .SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING)
                    .unwrap();
            }

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
                    x: -self.font_size.0 * col_offset as f32
                        + self.font_size.0 * (col + layout.col_offset) as f32,
                    y: self.font_size.1 * (row + layout.row_offset) as f32,
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
        layout: &RenderLayout,
        text: &[u8],
        effects: &[TextEffect],
        theme: &Theme,
        align_right: bool,
    ) {
        self.draw_text_with_col_offset(row, col, layout, text, effects, theme, 0, align_right)
    }

    pub fn draw_text_fit_view(
        &self,
        view: &View,
        layout: &RenderLayout,
        text: &[u8],
        effects: &[TextEffect],
        theme: &Theme,
    ) {
        unsafe {
            self.render_target.PushAxisAlignedClip(
                &D2D_RECT_F {
                    left: layout.col_offset as f32 * self.font_size.0,
                    top: layout.row_offset as f32 * self.font_size.1,
                    right: (layout.col_offset + layout.num_cols) as f32 * self.font_size.0,
                    bottom: (layout.row_offset + layout.num_rows) as f32 * self.font_size.1,
                },
                D2D1_ANTIALIAS_MODE_ALIASED,
            );
        }
        self.draw_text_with_col_offset(0, 0, layout, text, effects, theme, view.col_offset, false);
        unsafe {
            self.render_target.PopAxisAlignedClip();
        }
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

    pub fn draw_popup_below(
        &self,
        row: usize,
        col: usize,
        layout: &RenderLayout,
        text: &[u8],
        outer_color: Color,
        inner_color: Color,
        effects: Option<&[TextEffect]>,
        theme: &Theme,
        restrict: bool,
    ) {
        self.set_word_wrapping(true);

        let (mut row_offset, col_offset) = (
            (row + layout.row_offset) as f32 * self.font_size.1,
            (col + layout.col_offset) as f32 * self.font_size.0,
        );

        let mut restricted_layout = *layout;

        if restrict {
            restricted_layout.num_rows =
                (self.window_size.1 / self.font_size.1).ceil() as usize / 2;
            restricted_layout.num_cols =
                (self.window_size.0 / self.font_size.0).ceil() as usize / 2;
        }

        let (width, height) = self.get_text_width_height(
            col_offset + self.font_size.1 * 0.25,
            row_offset + self.font_size.1 * 0.25,
            &restricted_layout,
            text,
        );

        let (width, height) = (
            ((width / self.font_size.0 as f64).round() as usize).min(restricted_layout.num_cols),
            ((height / self.font_size.1 as f64).round() as usize).min(restricted_layout.num_rows),
        );

        if row_offset + (height as f32 * self.font_size.1) > self.window_size.1 {
            row_offset -=
                (height as f32 * self.font_size.1) + self.font_size.1 * 0.5 + self.font_size.1;
        }

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
                    right: col_offset
                        + self.font_size.0 * width as f32
                        + self.font_size.1 * 0.5
                        + 0.5,
                    bottom: row_offset
                        + self.font_size.1 * height as f32
                        + self.font_size.1 * 0.5
                        + 0.5,
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

            self.render_target.FillRectangle(
                &D2D_RECT_F {
                    left: col_offset - 0.5 + self.font_size.1 * 0.125,
                    top: row_offset - 0.5 + self.font_size.1 * 0.125,
                    right: col_offset
                        + self.font_size.0 * width as f32
                        + self.font_size.1 * 0.375
                        + 0.5,
                    bottom: row_offset
                        + self.font_size.1 * height as f32
                        + self.font_size.1 * 0.375
                        + 0.5,
                },
                &inner_brush,
            );

            self.render_target.PushAxisAlignedClip(
                &D2D_RECT_F {
                    left: col_offset - 0.5 + self.font_size.1 * 0.125,
                    top: row_offset - 0.5 + self.font_size.1 * 0.125,
                    right: col_offset
                        + self.font_size.0 * width as f32
                        + self.font_size.1 * 0.375
                        + 0.5,
                    bottom: row_offset
                        + self.font_size.1 * height as f32
                        + self.font_size.1 * 0.375
                        + 0.5,
                },
                D2D1_ANTIALIAS_MODE_ALIASED,
            );
        }

        self.draw_text_with_offset(
            col_offset + self.font_size.1 * 0.25,
            row_offset + self.font_size.1 * 0.25,
            &restricted_layout,
            text,
            effects.unwrap_or(&[]),
            theme,
        );

        self.set_word_wrapping(false);

        unsafe {
            self.render_target.PopAxisAlignedClip();
        }
    }

    pub fn draw_popup_above(
        &self,
        row: usize,
        col: usize,
        layout: &RenderLayout,
        text: &[u8],
        outer_color: Color,
        inner_color: Color,
        effects: Option<&[TextEffect]>,
        theme: &Theme,
        restrict: bool,
    ) {
        self.set_word_wrapping(true);

        let (mut row_offset, col_offset) = (
            (row + layout.row_offset) as f32 * self.font_size.1,
            (col + layout.col_offset) as f32 * self.font_size.0,
        );

        let mut restricted_layout = *layout;

        if restrict {
            restricted_layout.num_rows =
                (self.window_size.1 / self.font_size.1).ceil() as usize / 2;
            restricted_layout.num_cols =
                (self.window_size.0 / self.font_size.0).ceil() as usize / 2;
        }

        let (width, height) = self.get_text_width_height(
            col_offset + self.font_size.1 * 0.25,
            row_offset + self.font_size.1 * 0.25,
            &restricted_layout,
            text,
        );

        let (width, height) = (
            ((width / self.font_size.0 as f64).round() as usize).min(restricted_layout.num_cols),
            ((height / self.font_size.1 as f64).round() as usize).min(restricted_layout.num_rows),
        );

        if row_offset - (height as f32 * self.font_size.1) > 0.0 {
            row_offset -=
                (height as f32 * self.font_size.1) + self.font_size.1 * 0.5 + self.font_size.1;
        }

        unsafe {
            self.render_target.PushAxisAlignedClip(
                &D2D_RECT_F {
                    left: col_offset - 0.5,
                    top: row_offset - 0.5,
                    right: col_offset
                        + self.font_size.0 * width as f32
                        + self.font_size.1 * 0.5
                        + 0.5,
                    bottom: row_offset
                        + self.font_size.1 * height as f32
                        + self.font_size.1 * 0.5
                        + 0.5,
                },
                D2D1_ANTIALIAS_MODE_ALIASED,
            );

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
                    right: col_offset
                        + self.font_size.0 * width as f32
                        + self.font_size.1 * 0.5
                        + 0.5,
                    bottom: row_offset
                        + self.font_size.1 * height as f32
                        + self.font_size.1 * 0.5
                        + 0.5,
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

            self.render_target.FillRectangle(
                &D2D_RECT_F {
                    left: col_offset - 0.5 + self.font_size.1 * 0.125,
                    top: row_offset - 0.5 + self.font_size.1 * 0.125,
                    right: col_offset
                        + self.font_size.0 * width as f32
                        + self.font_size.1 * 0.375
                        + 0.5,
                    bottom: row_offset
                        + self.font_size.1 * height as f32
                        + self.font_size.1 * 0.375
                        + 0.5,
                },
                &inner_brush,
            );
        }

        self.draw_text_with_offset(
            col_offset + self.font_size.1 * 0.25,
            row_offset + self.font_size.1 * 0.25,
            &restricted_layout,
            text,
            effects.unwrap_or(&[]),
            theme,
        );

        self.set_word_wrapping(false);

        unsafe {
            self.render_target.PopAxisAlignedClip();
        }
    }

    pub fn draw_completion_popup(
        &self,
        row: usize,
        col: usize,
        layout: &RenderLayout,
        search_string: &str,
        selection_view_index: usize,
        text: &[u8],
        outer_color: Color,
        inner_color: Color,
        effects: Option<&[TextEffect]>,
        theme: &Theme,
    ) {
        self.set_word_wrapping(true);

        let (mut row_offset, col_offset) = (
            (row + layout.row_offset) as f32 * self.font_size.1,
            (col + layout.col_offset) as f32 * self.font_size.0,
        );

        let (width, height) = self.get_text_width_height(
            col_offset + self.font_size.1 * 0.25,
            row_offset + self.font_size.1 * 0.25,
            layout,
            text,
        );

        let width = width.max(
            self.get_text_width_height(
                col_offset + self.font_size.1 * 0.25,
                row_offset + self.font_size.1 * 0.25,
                layout,
                search_string.as_bytes(),
            )
            .0,
        );

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
                    right: col_offset
                        + self.font_size.0 * width as f32
                        + self.font_size.1 * 0.5
                        + 0.5,
                    bottom: row_offset
                        + self.font_size.1 * height as f32
                        + self.font_size.1 * 0.5
                        + 0.5,
                },
                &outer_brush,
            );
        }

        unsafe {
            let inner_brush = self
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

            self.render_target.FillRectangle(
                &D2D_RECT_F {
                    left: col_offset - 0.5 + self.font_size.1 * 0.125,
                    top: row_offset - 0.5 + self.font_size.1 * 0.125,
                    right: col_offset
                        + self.font_size.0 * width as f32
                        + self.font_size.1 * 0.375
                        + 0.5,
                    bottom: row_offset + self.font_size.1 + self.font_size.1 * 0.125,
                },
                &inner_brush,
            );
        }

        self.draw_text_with_offset(
            col_offset + self.font_size.1 * 0.25,
            row_offset + self.font_size.1 * 0.125,
            layout,
            search_string.as_bytes(),
            &[TextEffect {
                kind: TextEffectKind::ForegroundColor(theme.background_color),
                start: 0,
                length: search_string.len(),
            }],
            theme,
        );

        row_offset += self.font_size.1 + 0.5;

        unsafe {
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

            self.render_target.FillRectangle(
                &D2D_RECT_F {
                    left: col_offset - 0.5 + self.font_size.1 * 0.125,
                    top: row_offset - 0.5 + self.font_size.1 * 0.125,
                    right: col_offset
                        + self.font_size.0 * width as f32
                        + self.font_size.1 * 0.375
                        + 0.5,
                    bottom: row_offset
                        + self.font_size.1 * (height.saturating_sub(1)) as f32
                        + self.font_size.1 * 0.375
                        + 0.5,
                },
                &inner_brush,
            );

            let inner_brush = self
                .render_target
                .CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: theme.active_search_background_color.r,
                        g: theme.active_search_background_color.g,
                        b: theme.active_search_background_color.b,
                        a: 1.0,
                    },
                    Some(&DEFAULT_BRUSH_PROPERTIES),
                )
                .unwrap();

            self.render_target.FillRectangle(
                &D2D_RECT_F {
                    left: col_offset - 0.5 + self.font_size.1 * 0.125,
                    top: row_offset + self.font_size.1 * selection_view_index as f32 - 0.5
                        + self.font_size.1 * 0.25,
                    right: col_offset
                        + self.font_size.0 * width as f32
                        + self.font_size.1 * 0.375
                        + 0.5,
                    bottom: row_offset
                        + self.font_size.1 * (selection_view_index + 1) as f32
                        + self.font_size.1 * 0.25
                        + 0.5,
                },
                &inner_brush,
            );
        }

        self.draw_text_with_offset(
            col_offset + self.font_size.1 * 0.25,
            row_offset + self.font_size.1 * 0.25,
            layout,
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
