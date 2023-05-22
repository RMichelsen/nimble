use std::{ffi::c_void, mem::size_of, ptr::null, str::FromStr};

use core_foundation::{
    attributed_string::{CFAttributedString, CFAttributedStringSetAttribute},
    base::{CFRange, ToVoid},
    string::CFString,
};
use core_graphics::{
    color::CGColor,
    context::CGContext,
    display::CGRectInfinite,
    geometry::{CGPoint, CGRect, CGSize},
    path::CGPath, base::CGFloat,
};
use core_text::{
    framesetter::CTFramesetter,
    string_attributes::{kCTFontAttributeName, kCTParagraphStyleAttributeName},
};
use objc::{class, msg_send, runtime::Object, sel, sel_impl};
use winit::window::Window;
use std::cmp::min;

use crate::{
    renderer::{Color, RenderLayout, TextEffect, TextEffectKind},
    view::View,
    theme::Theme,
};

extern "C" {
    fn CTParagraphStyleCreate(
        settings: *const CTParagraphStyleSetting,
        settings_count: usize,
    ) -> *const c_void;
    fn CTFontGetAdvancesForGlyphs(
        font: *const c_void,
        orientation: u32,
        glyphs: *const u16,
        advances: *const c_void,
        count: i64,
    ) -> f64;
    fn CTFontGetBoundingRectsForGlyphs(
        font: *const c_void,
        orientation: u32,
        glyphs: *const u16,
        bounding_rects: *const c_void,
        count: i64,
    ) -> CGRect;
}

#[repr(C)]
struct CTParagraphStyleSetting {
    spec: u32,
    value_size: usize,
    value: *const c_void,
}
const LINEBREAK_SETTING_SPEC: u32 = 6;
const LINE_SPACING_SETTING_SPEC: u32 = 16;
const NO_WRAPPING_LINEBREAK_SETTING: u8 = 2;
const NO_WRAPPING_PARAGRAPH_STYLE: CTParagraphStyleSetting = CTParagraphStyleSetting {
    spec: LINEBREAK_SETTING_SPEC,
    value: &NO_WRAPPING_LINEBREAK_SETTING as *const u8 as *const c_void,
    value_size: size_of::<u8>(),
};

pub struct GraphicsContext {
    window_size: (f64, f64),
    paragraph_style: *const c_void,
    font: *mut c_void,
    pub font_size: (f64, f64),
}

impl GraphicsContext {
    pub fn new(window: &Window) -> Self {
        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );

        let font: *mut c_void =
            unsafe { msg_send![class!(NSFont), monospacedSystemFontOfSize:18.0 weight:0.0 ] };

        let font_size = unsafe {
            (
                CTFontGetAdvancesForGlyphs(font, 0, &(b'M' as u16) as *const u16, null(), 1),
                CTFontGetBoundingRectsForGlyphs(font, 0, &(b'M' as u16) as *const u16, null(), 1)
                    .size
                    .height
                    .round(),
            )
        };

        let line_spacing_paragraph_style = CTParagraphStyleSetting {
            spec: LINE_SPACING_SETTING_SPEC,
            value: &0.0 as *const f64 as *const c_void,
            value_size: size_of::<f64>(),
        };

        let settings = [NO_WRAPPING_PARAGRAPH_STYLE, line_spacing_paragraph_style];
        let paragraph_style = unsafe { CTParagraphStyleCreate(settings.as_ptr(), settings.len()) };

        Self {
            window_size,
            paragraph_style,
            font,
            font_size,
        }
    }

    pub fn ensure_size(&mut self, window: &Window) {
        self.window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );
    }

    pub fn begin_draw(&self) {}

    pub fn end_draw(&self) {}

    pub fn clear(&self, color: Color) {
        let context = get_current_context();
        context.set_fill_color(&CGColor::rgb(
            color.r as f64,
            color.g as f64,
            color.b as f64,
            1.0,
        ));
        context.fill_rect(unsafe { CGRectInfinite });
    }

    pub fn fill_cells(&self, row: usize, col: usize, layout: &RenderLayout, size: (usize, usize), color: Color) {
        let context = get_current_context();

        let (row_offset, col_offset) = (
            (row + layout.row_offset) as f64 * self.font_size.1,
            (col + layout.col_offset) as f64 * self.font_size.0,
        );

        context.set_fill_color(&CGColor::rgb(
            color.r as f64,
            color.g as f64,
            color.b as f64,
            1.0,
        ));

        context.fill_rect(CGRect::new(
            &CGPoint::new(
                col_offset,
                self.window_size.1 - (self.font_size.1 * size.1 as f64) - row_offset,
            ),
            &CGSize::new(
                self.font_size.0 * size.0 as f64,
                self.font_size.1 * size.1 as f64,
            ),
        ));
    }

    pub fn fill_cell_slim_line(&self, row: usize, col: usize, layout: &RenderLayout, color: Color) {
        let context = get_current_context();

        let (row_offset, col_offset) = (
            (row + layout.row_offset) as f64 * self.font_size.1,
            (col + layout.col_offset) as f64 * self.font_size.0,
        );

        context.set_fill_color(&CGColor::rgb(
            color.r as f64,
            color.g as f64,
            color.b as f64,
            1.0,
        ));

        context.fill_rect(CGRect::new(
            &CGPoint::new(
                col_offset,
                self.window_size.1 - self.font_size.1 - row_offset,
            ),
            &CGSize::new(self.font_size.0 * 0.1, self.font_size.1),
        ));
    }

    pub fn underline_cells(&self, row: usize, col: usize, layout: &RenderLayout, count: usize, color: Color) {
        let context = get_current_context();

        let (row_offset, col_offset) = (
            (row + layout.row_offset) as f64 * self.font_size.1,
            (col + layout.col_offset) as f64 * self.font_size.0,
        );

        context.set_fill_color(&CGColor::rgb(
            color.r as f64,
            color.g as f64,
            color.b as f64,
            1.0,
        ));

        context.fill_rect(CGRect::new(
            &CGPoint::new(
                col_offset,
                self.window_size.1 - self.font_size.1 - row_offset,
            ),
            &CGSize::new(
                self.font_size.0 * count as f64,
                self.font_size.1 * 0.1 as f64,
            ),
        ));
    }

    fn get_text_size(
        &self,
        x: f64,
        y: f64,
        layout: &RenderLayout,
        text: &[u8]
    ) -> CGSize {
        let utf8_str = unsafe { std::str::from_utf8_unchecked(text) };
        let string = CFAttributedString::new(
            &CFString::from_str(utf8_str).unwrap(),
        );

        unsafe {
            CFAttributedStringSetAttribute(
                string.to_void() as *const _,
                CFRange::init(0, string.char_len()),
                kCTFontAttributeName,
                self.font,
            );
        };

        let framesetter = CTFramesetter::new_with_attributed_string(string.to_void() as *const _);
        let size = framesetter.suggest_frame_size_with_constraints(
            CFRange::init(0, string.char_len()), null(), CGSize { 
                width: (self.font_size.0 * layout.num_cols as f64 - x).clamp(0.0, f64::MAX),
                height: (self.font_size.1 * layout.num_rows as f64 - y).clamp(0.0, f64::MAX)
            });

        size.0
    }

    fn draw_text_with_offset(
        &self,
        x: f64,
        y: f64,
        layout: &RenderLayout,
        text: &[u8],
        effects: &[TextEffect],
        theme: &Theme,
    ) {
        let utf8_str = unsafe { std::str::from_utf8_unchecked(text) };
        let string = CFAttributedString::new(
            &CFString::from_str(utf8_str).unwrap(),
        );
        let string_len = utf8_str.len();

        unsafe {
            let text_color =
                CGColor::rgb(theme.foreground_color.r as f64,
                    theme.foreground_color.g as f64,
                    theme.foreground_color.b as f64,
                    1.0);
            CFAttributedStringSetAttribute(
                string.to_void() as *const _,
                CFRange::init(0, string.char_len()),
                core_text::string_attributes::kCTForegroundColorAttributeName,
                text_color.to_void() as *const _,
            );
        }

        for effect in effects {
            match &effect.kind {
                TextEffectKind::ForegroundColor(color) => {
                    let text_color =
                        CGColor::rgb(color.r as f64, color.g as f64, color.b as f64, 1.0);
                    unsafe {
                        CFAttributedStringSetAttribute(
                            string.to_void() as *const _,
                            CFRange::init(effect.start as isize, min(string_len.saturating_sub(effect.start), effect.length) as isize),
                            core_text::string_attributes::kCTForegroundColorAttributeName,
                            text_color.to_void() as *const _,
                        );
                    };
                }
            }
        }

        unsafe {
            CFAttributedStringSetAttribute(
                string.to_void() as *const _,
                CFRange::init(0, string.char_len()),
                kCTFontAttributeName,
                self.font,
            );
        };

        let context = get_current_context();

        let size = self.get_text_size(x, y, layout, text);
        use bstr::ByteSlice;
        println!("font: {}, size: {:?}, text: {}", self.font_size.0, size, text.as_bstr());

        let framesetter = CTFramesetter::new_with_attributed_string(string.to_void() as *const _);

        let bounding_rect = CGPath::from_rect(
            CGRect {
                origin: CGPoint { x, y: self.window_size.1 - size.height - y },
                size,
            },
            None,
        );

        let frame = framesetter.create_frame(CFRange::init(0, string.char_len()), &bounding_rect);
        frame.draw(&context);
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
        let utf8_str = unsafe { std::str::from_utf8_unchecked(text) };
        let string = CFAttributedString::new(
            &CFString::from_str(utf8_str).unwrap(),
        );
        let string_len = utf8_str.len();

        for effect in effects {
            match &effect.kind {
                TextEffectKind::ForegroundColor(color) => {
                    let text_color =
                        CGColor::rgb(color.r as f64, color.g as f64, color.b as f64, 1.0);
                    unsafe {
                        CFAttributedStringSetAttribute(
                            string.to_void() as *const _,
                            CFRange::init(effect.start as isize, min(string_len.saturating_sub(effect.start), effect.length) as isize),
                            core_text::string_attributes::kCTForegroundColorAttributeName,
                            text_color.to_void() as *const _,
                        );
                    };
                }
            }
        }

        unsafe {
            CFAttributedStringSetAttribute(
                string.to_void() as *const _,
                CFRange::init(0, string.char_len()),
                kCTFontAttributeName,
                self.font,
            );

            CFAttributedStringSetAttribute(
                string.to_void() as *const _,
                CFRange::init(0, string.char_len()),
                kCTParagraphStyleAttributeName,
                self.paragraph_style as *const _,
            );
        };

        let context = get_current_context();

        let bounding_rect = CGPath::from_rect(
            CGRect {
                origin: CGPoint {
                    x: -self.font_size.0 * col_offset as f64 + self.font_size.0 * (col + layout.col_offset) as f64,
                    y: self.window_size.1 - (self.font_size.1 * layout.num_rows as f64) - self.font_size.1 * (row + layout.row_offset) as f64,
                },
                size: CGSize {
                    width: self.font_size.0 * layout.num_cols as f64,
                    height: self.font_size.1 * layout.num_rows as f64,
                },
            },
            None,
        );

        let ct_typesetter = CTFramesetter::new_with_attributed_string(string.to_void() as *const _);
        let frame = ct_typesetter.create_frame(CFRange::init(0, string.char_len()), &bounding_rect);
        frame.draw(&context);
    }

    pub fn draw_text(&self, row: usize, col: usize, layout: &RenderLayout, text: &[u8], effects: &[TextEffect], theme: &Theme,
        align_right: bool) {
        self.draw_text_with_col_offset(row, col, layout, text, effects, theme, 0, align_right)
    }

    pub fn draw_text_fit_view(&self, view: &View, layout: &RenderLayout, text: &[u8], effects: &[TextEffect], theme: &Theme) {
        self.draw_text_with_col_offset(0, 0, layout, text, effects, theme, view.col_offset, false)
    }

    pub fn set_word_wrapping(&self, wrap: bool) {
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
    ) {
        let (mut row_offset, col_offset) = (
            (row + layout.row_offset) as f64 * self.font_size.1,
            (col + layout.col_offset) as f64 * self.font_size.0,
        );

        self.draw_text_with_offset(
            col_offset + self.font_size.1 * 0.5,
            row_offset - self.font_size.1 * 0.5,
            layout,
            text,
            effects.unwrap_or(&[]),
            theme,
        );
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
    ) {
        // self.set_word_wrapping(true);

        let (mut row_offset, col_offset) = (
            (row + layout.row_offset) as f64 * self.font_size.1,
            (col + layout.col_offset) as f64 * self.font_size.0,
        );

        // let (width, height) = self.get_text_bounding_box(
        //     col_offset + self.font_size.1 * 0.5,
        //     row_offset + self.font_size.1 * 0.5,
        //     layout,
        //     text,
        // );

        // if row_offset - height as f32 > 0.0 {
        //     row_offset -= height as f32 + self.font_size.1 + self.font_size.1;
        // }

        // let (width, height) = (
        //     (width / self.font_size.0 as f64).round() as usize,
        //     (height / self.font_size.1 as f64).round() as usize,
        // );

        unsafe {
            // let outer_brush = self
            //     .render_target
            //     .CreateSolidColorBrush(
            //         &D2D1_COLOR_F {
            //             r: outer_color.r,
            //             g: outer_color.g,
            //             b: outer_color.b,
            //             a: 1.0,
            //         },
            //         Some(&DEFAULT_BRUSH_PROPERTIES),
            //     )
            //     .unwrap();

            // self.render_target.FillRectangle(
            //     &D2D_RECT_F {
            //         left: col_offset - 0.5,
            //         top: row_offset - 0.5,
            //         right: col_offset + self.font_size.0 * width as f32 + self.font_size.1 + 0.5,
            //         bottom: row_offset + self.font_size.1 * height as f32 + self.font_size.1 + 0.5,
            //     },
            //     &outer_brush,
            // );

            // context.fill_rect(CGRect::new(
            //     &CGPoint::new(
            //         col_offset,
            //         self.window_size.1 - (self.font_size.1 * height) - row_offset,
            //     ),
            //     &CGSize::new(
            //         self.font_size.0 * count as f64,
            //         self.font_size.1 * 0.1 as f64,
            //     ),
            // ));

            // let inner_brush = self
            //     .render_target
            //     .CreateSolidColorBrush(
            //         &D2D1_COLOR_F {
            //             r: inner_color.r,
            //             g: inner_color.g,
            //             b: inner_color.b,
            //             a: 1.0,
            //         },
            //         Some(&DEFAULT_BRUSH_PROPERTIES),
            //     )
            //     .unwrap();

            // self.render_target.FillRoundedRectangle(
            //     &D2D1_ROUNDED_RECT {
            //         rect: D2D_RECT_F {
            //             left: col_offset - 0.5 + self.font_size.1 * 0.25,
            //             top: row_offset - 0.5 + self.font_size.1 * 0.25,
            //             right: col_offset
            //                 + self.font_size.0 * width as f32
            //                 + self.font_size.1 * 0.75
            //                 + 0.5,
            //             bottom: row_offset
            //                 + self.font_size.1 * height as f32
            //                 + self.font_size.1 * 0.75
            //                 + 0.5,
            //         },
            //         radiusX: 1.5,
            //         radiusY: 1.5,
            //     },
            //     &inner_brush,
            // );
        }

        self.draw_text_with_offset(
            col_offset + self.font_size.1 * 0.5,
            row_offset + self.font_size.1 * 0.5,
            layout,
            text,
            effects.unwrap_or(&[]),
            theme,
        );

        // self.set_word_wrapping(false);
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
        let (mut row_offset, col_offset) = (
            (row + layout.row_offset) as f64 * self.font_size.1,
            (col + layout.col_offset) as f64 * self.font_size.0,
        );

        let size = self.get_text_size(
            col_offset + self.font_size.1 * 0.5,
            row_offset - self.font_size.1 * 0.5,
            layout,
            text,
        );

        let width = size.width.max(
            self.get_text_size(
                col_offset + self.font_size.1 * 0.5,
                row_offset - self.font_size.1 * 0.5,
                layout,
                search_string.as_bytes(),
            )
            .width,
        );

        let (width, height) = (
            (width / self.font_size.0 as f64).round() as usize,
            (size.height / self.font_size.1 as f64).round() as usize,
        );


        let context = get_current_context();
        context.set_fill_color(&CGColor::rgb(
            outer_color.r as f64,
            outer_color.g as f64,
            outer_color.b as f64,
            1.0,
        ));
        context.fill_rect(
            CGRect { 
                origin: CGPoint { x: col_offset, y: self.window_size.1 - self.font_size.1 * (height + 1) as f64 - row_offset },
                size: CGSize { 
                    width: self.font_size.0 * width as f64 + self.font_size.1,
                    height: self.font_size.1 * (height + 1) as f64
                }
            }
        );

        context.set_fill_color(&CGColor::rgb(
            theme.foreground_color.r as f64,
            theme.foreground_color.g as f64,
            theme.foreground_color.b as f64,
            1.0,
        ));
        context.fill_rect(
            CGRect { 
                origin: CGPoint { x: col_offset + self.font_size.1 * 0.25, y: self.window_size.1 - self.font_size.1 * 1.5 - row_offset},
                size: CGSize { 
                    width: self.font_size.0 * width as f64 + self.font_size.1 * 0.5,
                    height: self.font_size.1 * 1.25,
                }
            }
        );

        self.draw_text_with_offset(
            col_offset + self.font_size.1 * 0.5,
            row_offset - self.font_size.1 * 0.25,
            layout,
            search_string.as_bytes(),
            &[TextEffect {
                kind: TextEffectKind::ForegroundColor(theme.background_color),
                start: 0,
                length: search_string.len(),
            }],
            theme,
        );

        row_offset += self.font_size.1;

        self.draw_text_with_offset(
            col_offset + self.font_size.1 * 0.5,
            row_offset - self.font_size.1 * 0.5,
            layout,
            text,
            effects.unwrap_or(&[]),
            theme,
        );
    }
}

fn get_current_context() -> CGContext {
    let graphics_context: *mut Object =
        unsafe { msg_send![class![NSGraphicsContext], currentContext] };
    let cg_context: *mut Object = unsafe { msg_send![graphics_context, CGContext] };
    unsafe { CGContext::from_existing_context_ptr(cg_context as *mut _) }
}
