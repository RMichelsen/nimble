use std::{cell::RefCell, rc::Rc};

use bstr::ByteSlice;
use winit::window::Window;

use crate::{
    buffer::{Buffer, BufferMode},
    graphics_context::GraphicsContext,
    language_server::LanguageServer,
    text_utils,
    theme::{
        BACKGROUND_COLOR, COMMENT_COLOR, CURSOR_COLOR, HIGHLIGHT_COLOR, KEYWORD_COLOR, TEXT_COLOR,
    },
    view::View,
};

pub enum TextEffectKind {
    ForegroundColor(Color),
}

pub struct TextEffect {
    pub kind: TextEffectKind,
    pub start: usize,
    pub length: usize,
}

pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

pub struct Renderer {
    context: GraphicsContext,
    window_size: (f64, f64),
    pub num_rows: usize,
    pub num_cols: usize,
}

impl Renderer {
    pub fn new(window: &Window) -> Self {
        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );
        let context = GraphicsContext::new(window);
        let num_rows = (window_size.1 / context.font_size.1 as f64).ceil() as usize;
        let num_cols = (window_size.0 / context.font_size.0 as f64).ceil() as usize;
        Self {
            context,
            window_size,
            num_rows,
            num_cols,
        }
    }

    pub fn draw_buffer(
        &mut self,
        buffer: &Buffer,
        view: &View,
        language_server: &Option<Rc<RefCell<LanguageServer>>>,
    ) {
        use TextEffectKind::*;

        self.context.begin_draw();
        self.context.clear(BACKGROUND_COLOR);

        if buffer.mode != BufferMode::Insert {
            view.visible_cursors_iter(buffer, self.num_rows, self.num_cols, |row, col, num| {
                self.context.fill_cells(row, col, (num, 1), HIGHLIGHT_COLOR);
            });
        }

        view.visible_cursor_leads_iter(buffer, self.num_rows, self.num_cols, |row, col| {
            if buffer.mode == BufferMode::Insert {
                self.context.fill_cell_slim_line(row, col, CURSOR_COLOR);
            } else {
                self.context.fill_cells(row, col, (1, 1), CURSOR_COLOR);
            }
        });

        let text = view.visible_text(buffer, self.num_rows);

        let mut effects = vec![];
        effects.push(TextEffect {
            kind: ForegroundColor(TEXT_COLOR),
            start: 0,
            length: text.len(),
        });
        if let Some(keywords) = &buffer.language.keywords {
            text_utils::find_keywords_iter(&text, keywords, |start, len| {
                effects.push(TextEffect {
                    kind: ForegroundColor(KEYWORD_COLOR),
                    start,
                    length: len,
                })
            });
        }
        if let Some(line_comment_tokens) = buffer.language.line_comment_token {
            if let Some(idx) = &text.find(line_comment_tokens) {
                effects.push(TextEffect {
                    kind: ForegroundColor(COMMENT_COLOR),
                    start: *idx,
                    length: text.len() - idx,
                })
            }
        }

        self.context.draw_text_fit_view(view, &text, &effects);

        view.visible_completions(
            buffer,
            self.num_rows,
            self.num_cols,
            |completion, completion_view, request| {
                let selected_item = request.selection_index - request.selection_view_offset;

                self.context.fill_cells(
                    completion_view.row,
                    completion_view.col,
                    (completion_view.width, completion_view.height),
                    HIGHLIGHT_COLOR,
                );
                self.context.fill_cells(
                    completion_view.row + selected_item,
                    completion_view.col,
                    (completion_view.width, 1),
                    CURSOR_COLOR,
                );

                let mut selected_item_start_position = 0;
                let mut completion_string = String::default();
                for (i, item) in completion
                    .items
                    .iter()
                    .enumerate()
                    .skip(request.selection_view_offset)
                    .take(completion_view.height)
                {
                    if i - request.selection_view_offset == selected_item {
                        selected_item_start_position = completion_string.len()
                    }

                    completion_string.push_str(item.insert_text.as_ref().unwrap_or(&item.label));
                    completion_string.push('\n');
                }

                let effects = [
                    TextEffect {
                        kind: ForegroundColor(TEXT_COLOR),
                        start: 0,
                        length: completion_string.len(),
                    },
                    TextEffect {
                        kind: ForegroundColor(KEYWORD_COLOR),
                        start: selected_item_start_position,
                        length: completion.items[request.selection_index].label.len(),
                    },
                ];
                self.context.draw_text(
                    completion_view.row,
                    completion_view.col,
                    completion_string.as_bytes(),
                    &effects,
                );
            },
        );

        self.context.end_draw();
    }
}
