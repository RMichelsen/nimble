use std::{cell::RefCell, rc::Rc};

use bstr::ByteSlice;
use widestring::{u16str, U16CString};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::HWND,
        Graphics::{
            Direct2D::{
                Common::{
                    D2D1_ALPHA_MODE_IGNORE, D2D1_PIXEL_FORMAT, D2D_POINT_2F, D2D_RECT_F, D2D_SIZE_U,
                },
                D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
                D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
                D2D1_FEATURE_LEVEL_DEFAULT, D2D1_HWND_RENDER_TARGET_PROPERTIES,
                D2D1_PRESENT_OPTIONS_IMMEDIATELY, D2D1_RENDER_TARGET_PROPERTIES,
                D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE,
            },
            DirectWrite::{
                DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
                DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_HIT_TEST_METRICS, DWRITE_TEXT_RANGE, DWRITE_WORD_WRAPPING_NO_WRAP,
            },
            Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
        },
    },
};
use winit::{platform::windows::WindowExtWindows, window::Window};

use crate::{
    buffer::{Buffer, BufferMode},
    cursor::NUM_SHOWN_COMPLETION_ITEMS,
    language_server::LanguageServer,
    text_utils,
    theme::{
        BACKGROUND_COLOR, COMMENT_COLOR, CURSOR_COLOR, DEFAULT_BRUSH_PROPERTIES, HIGHLIGHT_COLOR,
        KEYWORD_COLOR, TEXT_COLOR,
    },
    view::View,
};

pub struct Renderer {
    render_target: ID2D1HwndRenderTarget,
    dwrite_factory: IDWriteFactory,
    text_format: IDWriteTextFormat,
    text_brush: ID2D1SolidColorBrush,
    highlight_brush: ID2D1SolidColorBrush,
    cursor_brush: ID2D1SolidColorBrush,
    keyword_brush: ID2D1SolidColorBrush,
    comment_brush: ID2D1SolidColorBrush,
    font_size: (f32, f32),
    window_size: (f32, f32),
    pub num_rows: usize,
    pub num_cols: usize,
}

impl Renderer {
    pub fn new(window: &Window) -> Self {
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
                    20.0,
                    PCWSTR(U16CString::from_str("en-us").unwrap().into_raw()),
                )
                .unwrap()
        };
        unsafe {
            text_format
                .SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)
                .unwrap();
        }

        let text_brush = unsafe {
            render_target
                .CreateSolidColorBrush(&TEXT_COLOR, Some(&DEFAULT_BRUSH_PROPERTIES))
                .unwrap()
        };
        let keyword_brush = unsafe {
            render_target
                .CreateSolidColorBrush(&KEYWORD_COLOR, Some(&DEFAULT_BRUSH_PROPERTIES))
                .unwrap()
        };
        let highlight_brush = unsafe {
            render_target
                .CreateSolidColorBrush(&HIGHLIGHT_COLOR, Some(&DEFAULT_BRUSH_PROPERTIES))
                .unwrap()
        };
        let cursor_brush = unsafe {
            render_target
                .CreateSolidColorBrush(&CURSOR_COLOR, Some(&DEFAULT_BRUSH_PROPERTIES))
                .unwrap()
        };
        let comment_brush = unsafe {
            render_target
                .CreateSolidColorBrush(&COMMENT_COLOR, Some(&DEFAULT_BRUSH_PROPERTIES))
                .unwrap()
        };

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

        let window_size = (
            window.inner_size().width as f32 / window.scale_factor() as f32,
            window.inner_size().height as f32 / window.scale_factor() as f32,
        );

        Self {
            dwrite_factory,
            render_target,
            text_format,
            text_brush,
            keyword_brush,
            highlight_brush,
            cursor_brush,
            comment_brush,
            font_size,
            window_size,
            num_rows: (window_size.1 / font_size.1).ceil() as usize,
            num_cols: (window_size.0 / font_size.0).ceil() as usize,
        }
    }

    pub fn draw_buffer(
        &mut self,
        buffer: &Buffer,
        view: &View,
        language_server: &Option<Rc<RefCell<LanguageServer>>>,
    ) {
        unsafe {
            self.render_target.BeginDraw();
            self.render_target.Clear(Some(&BACKGROUND_COLOR)); //asdasdasds
        }

        if buffer.mode != BufferMode::Insert {
            view.visible_cursors_iter(
                buffer,
                self.num_rows,
                self.num_cols,
                |row, col, num| unsafe {
                    let (row_offset, col_offset) =
                        (row as f32 * self.font_size.1, col as f32 * self.font_size.0);
                    self.render_target.FillRectangle(
                        &D2D_RECT_F {
                            left: col_offset - 0.5,
                            top: row_offset - 0.5,
                            right: col_offset + self.font_size.0 * num as f32 + 0.5,
                            bottom: row_offset + self.font_size.1 + 0.5,
                        },
                        &self.highlight_brush,
                    );
                },
            );
        }

        view.visible_cursor_leads_iter(buffer, self.num_rows, self.num_cols, |row, col| unsafe {
            let (row_offset, col_offset) =
                (row as f32 * self.font_size.1, col as f32 * self.font_size.0);
            if buffer.mode == BufferMode::Insert {
                self.render_target.FillRectangle(
                    &D2D_RECT_F {
                        left: col_offset,
                        top: row_offset,
                        right: col_offset + self.font_size.0 * 0.1,
                        bottom: row_offset + self.font_size.1,
                    },
                    &self.cursor_brush,
                );
            } else {
                self.render_target.FillRectangle(
                    &D2D_RECT_F {
                        left: col_offset,
                        top: row_offset,
                        right: col_offset + self.font_size.0,
                        bottom: row_offset + self.font_size.1,
                    },
                    &self.cursor_brush,
                );
            }
        });

        view.visible_lines_iter(buffer, self.num_rows, self.num_cols, |i, line| {
            let text_layout = unsafe {
                self.dwrite_factory
                    .CreateTextLayout(
                        U16CString::from_str(std::str::from_utf8(line).unwrap())
                            .unwrap()
                            .as_slice(),
                        &self.text_format,
                        self.render_target.GetSize().width,
                        self.render_target.GetSize().height,
                    )
                    .unwrap()
            };

            if let Some(keywords) = &buffer.language.keywords {
                text_utils::find_keywords_iter(line, keywords, |start, len| unsafe {
                    text_layout
                        .SetDrawingEffect(
                            &self.keyword_brush,
                            DWRITE_TEXT_RANGE {
                                startPosition: start as u32,
                                length: len as u32,
                            },
                        )
                        .unwrap();
                });
            }

            if let Some(line_comment_tokens) = buffer.language.line_comment_token {
                if let Some(idx) = &line.find(line_comment_tokens) {
                    unsafe {
                        text_layout
                            .SetDrawingEffect(
                                &self.comment_brush,
                                DWRITE_TEXT_RANGE {
                                    startPosition: *idx as u32,
                                    length: (line.len() - *idx) as u32,
                                },
                            )
                            .unwrap();
                    }
                }
            }

            unsafe {
                self.render_target.DrawTextLayout(
                    D2D_POINT_2F {
                        x: self.font_size.0 * -(view.col_offset as f32),
                        y: self.font_size.1 * i as f32,
                    },
                    &text_layout,
                    &self.text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                );
            }
        });

        view.visible_completions(
            buffer,
            self.num_rows,
            self.num_cols,
            |row, col, request| unsafe {
                if let Some(server) = language_server {
                    if let Some(completions) = server.borrow().saved_completions.get(&request.id) {
                        if completions.items.is_empty() {
                            return;
                        }

                        let (row_offset, col_offset) =
                            (row as f32 * self.font_size.1, col as f32 * self.font_size.0);

                        let longest_string = completions
                            .items
                            .iter()
                            .max_by(|x, y| x.label.len().cmp(&y.label.len()))
                            .map(|x| x.label.len() + 1)
                            .unwrap_or(0);

                        let mut completion_string = String::default();
                        for item in completions
                            .items
                            .iter()
                            .skip(request.selection_view_offset)
                            .take(NUM_SHOWN_COMPLETION_ITEMS)
                        {
                            completion_string.push_str(&item.label);
                            completion_string.push('\n');
                        }
                        let selected_item = request.selection_index - request.selection_view_offset;

                        let completion_rect = D2D_RECT_F {
                            left: col_offset - 0.5,
                            top: row_offset + self.font_size.1 - 0.5,
                            right: col_offset + self.font_size.0 * longest_string as f32 + 0.5,
                            bottom: row_offset
                                + self.font_size.1 * (NUM_SHOWN_COMPLETION_ITEMS + 1) as f32
                                + 0.5,
                        };
                        self.render_target
                            .FillRectangle(&completion_rect, &self.highlight_brush);

                        let selected_completion_rect = D2D_RECT_F {
                            left: col_offset - 0.5,
                            top: row_offset + self.font_size.1 * (selected_item + 1) as f32 - 0.5,
                            right: col_offset + self.font_size.0 * longest_string as f32 + 0.5,
                            bottom: row_offset
                                + self.font_size.1 * (selected_item + 2) as f32
                                + 0.5,
                        };
                        self.render_target
                            .FillRectangle(&selected_completion_rect, &self.cursor_brush);

                        let start_position = completion_string
                            .find(completions.items[request.selection_index].label.as_str())
                            .unwrap() as u32;

                        let text_layout = self
                            .dwrite_factory
                            .CreateTextLayout(
                                U16CString::from_str(completion_string).unwrap().as_slice(),
                                &self.text_format,
                                completion_rect.right - completion_rect.left,
                                completion_rect.bottom - completion_rect.top,
                            )
                            .unwrap();

                        text_layout
                            .SetDrawingEffect(
                                &self.keyword_brush,
                                DWRITE_TEXT_RANGE {
                                    startPosition: start_position,
                                    length: completions.items[request.selection_index].label.len()
                                        as u32,
                                },
                            )
                            .unwrap();

                        self.render_target.DrawTextLayout(
                            D2D_POINT_2F {
                                x: completion_rect.left,
                                y: completion_rect.top,
                            },
                            &text_layout,
                            &self.text_brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                        );
                    }
                }
            },
        );

        unsafe {
            self.render_target.EndDraw(None, None).unwrap();
        }
    }
}
