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
    path::CGPath,
};
use core_text::{
    framesetter::CTFramesetter,
    string_attributes::{kCTFontAttributeName, kCTParagraphStyleAttributeName},
};
use objc::{class, msg_send, runtime::Object, sel, sel_impl};
use winit::window::Window;

use crate::{
    renderer::{Color, TextEffect, TextEffectKind},
    view::View,
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
            unsafe { msg_send![class!(NSFont), monospacedSystemFontOfSize:20.0 weight:0.0 ] };

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

    pub fn fill_cells(&self, row: usize, col: usize, size: (usize, usize), color: Color) {
        let context = get_current_context();
        let (row_offset, col_offset) =
            (row as f64 * self.font_size.1, col as f64 * self.font_size.0);
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

    pub fn fill_cell_slim_line(&self, row: usize, col: usize, color: Color) {
        let context = get_current_context();
        let (row_offset, col_offset) =
            (row as f64 * self.font_size.1, col as f64 * self.font_size.0);
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

    pub fn draw_text_with_col_offset(
        &self,
        row: usize,
        col: usize,
        text: &[u8],
        effects: &[TextEffect],
        col_offset: usize,
    ) {
        let string = CFAttributedString::new(
            &CFString::from_str(unsafe { std::str::from_utf8_unchecked(text) }).unwrap(),
        );

        for effect in effects {
            match &effect.kind {
                TextEffectKind::ForegroundColor(color) => {
                    let text_color =
                        CGColor::rgb(color.r as f64, color.g as f64, color.b as f64, 1.0);
                    unsafe {
                        CFAttributedStringSetAttribute(
                            string.to_void() as *const _,
                            CFRange::init(effect.start as isize, effect.length as isize),
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
                    x: -self.font_size.0 * col_offset as f64 + self.font_size.0 * col as f64,
                    y: -self.font_size.1 * row as f64,
                },
                size: CGSize {
                    width: self.window_size.0 + self.font_size.0 * col_offset as f64,
                    height: self.window_size.1,
                },
            },
            None,
        );

        let ct_typesetter = CTFramesetter::new_with_attributed_string(string.to_void() as *const _);
        let frame = ct_typesetter.create_frame(CFRange::init(0, string.char_len()), &bounding_rect);
        frame.draw(&context);
    }

    pub fn draw_text(&self, row: usize, col: usize, text: &[u8], effects: &[TextEffect]) {
        self.draw_text_with_col_offset(row, col, text, effects, 0)
    }

    pub fn draw_text_fit_view(&self, view: &View, text: &[u8], effects: &[TextEffect]) {
        self.draw_text_with_col_offset(0, 0, text, effects, view.col_offset)
    }
}

fn get_current_context() -> CGContext {
    let graphics_context: *mut Object =
        unsafe { msg_send![class![NSGraphicsContext], currentContext] };
    let cg_context: *mut Object = unsafe { msg_send![graphics_context, CGContext] };
    unsafe { CGContext::from_existing_context_ptr(cg_context as *mut _) }
}
