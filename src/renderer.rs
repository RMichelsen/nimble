use std::{cell::RefCell, rc::Rc};

use winit::window::Window;

use crate::{
    buffer::{Buffer, BufferMode},
    graphics_context::GraphicsContext,
    language_server::LanguageServer,
    text_utils::{comment_highlights, keyword_highlights, string_highlights},
    theme::{
        BACKGROUND_COLOR, CURSOR_COLOR, DIAGNOSTIC_COLOR, HIGHLIGHT_COLOR, KEYWORD_COLOR,
        TEXT_COLOR,
    },
    view::View,
};

#[derive(Debug)]
pub enum TextEffectKind {
    ForegroundColor(Color),
}

#[derive(Debug)]
pub struct TextEffect {
    pub kind: TextEffectKind,
    pub start: usize,
    pub length: usize,
}

#[derive(Debug)]
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
        let context = GraphicsContext::new(window);
        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );
        let num_rows = (window_size.1 / context.font_size.1 as f64).ceil() as usize;
        let num_cols = (window_size.0 / context.font_size.0 as f64).ceil() as usize;

        Self {
            context,
            window_size,
            num_rows,
            num_cols,
        }
    }

    pub fn get_font_size(&self) -> (f64, f64) {
        (
            self.context.font_size.0 as f64,
            self.context.font_size.1 as f64,
        )
    }

    pub fn start_draw(&self) {
        self.context.begin_draw();
        self.context.clear(BACKGROUND_COLOR);
    }

    pub fn end_draw(&self) {
        self.context.end_draw();
    }

    pub fn draw_buffer(
        &mut self,
        buffer: &Buffer,
        view: &View,
        language_server: &Option<Rc<RefCell<LanguageServer>>>,
    ) {
        use TextEffectKind::*;

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

        let keyword_highlights = keyword_highlights(&text, buffer.language.keywords);

        let comment_highlights = comment_highlights(
            &text,
            buffer.language.line_comment_token,
            buffer.language.multi_line_comment_token_pair,
            buffer.piece_table.iter_chars_at_rev(
                buffer
                    .piece_table
                    .char_index_from_line_col(view.line_offset, 0)
                    .unwrap(),
            ),
        );
        let string_highlights = string_highlights(&text, &comment_highlights);

        if let Some(server) = language_server {
            if let Some(diagnostics) = server
                .borrow()
                .saved_diagnostics
                .get(&buffer.uri.to_ascii_lowercase())
            {
                view.visible_diagnostic_lines_iter(
                    buffer,
                    diagnostics,
                    self.num_rows,
                    self.num_cols,
                    |row, col, count| {
                        self.context
                            .underline_cells(row, col, count, DIAGNOSTIC_COLOR);
                    },
                );

                if let Some((line, col)) = view.hover {
                    if let Some(diagnostic) = diagnostics.iter().find(|diagnostic| {
                        let (start_line, start_col) = (
                            diagnostic.range.start.line as usize,
                            diagnostic.range.start.character as usize,
                        );
                        let (end_line, end_col) = (
                            diagnostic.range.end.line as usize,
                            diagnostic.range.end.character as usize,
                        );
                        (start_line == line && (start_col..=end_col).contains(&col))
                            || (end_line == line && (start_col..=end_col).contains(&col))
                            || (diagnostic.range.start.line as usize
                                ..diagnostic.range.end.line as usize)
                                .contains(&line)
                    }) {
                        let (row, col) = (
                            view.absolute_to_view_row(line) + 1,
                            view.absolute_to_view_col(col),
                        );
                        self.context.fill_cells(
                            row,
                            col,
                            (diagnostic.message.len(), 1),
                            HIGHLIGHT_COLOR,
                        );
                        self.context
                            .draw_text(row, col, diagnostic.message.as_bytes(), &[])
                    }
                }
            }
        }

        effects.extend(keyword_highlights);
        effects.extend(comment_highlights);
        effects.extend(string_highlights);

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
                        selected_item_start_position = completion_string.len();
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
                        length: completion.items[request.selection_index]
                            .insert_text
                            .as_ref()
                            .unwrap_or(&completion.items[request.selection_index].label)
                            .len(),
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
    }
}
