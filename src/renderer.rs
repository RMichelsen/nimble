use std::{
    cell::RefCell,
    cmp::{max, min},
    rc::Rc,
    str::pattern::Pattern,
};

use winit::window::Window;

use crate::{
    buffer::{Buffer, BufferMode},
    editor::{FileFinder, MAX_SHOWN_FILE_FINDER_ITEMS},
    graphics_context::GraphicsContext,
    language_server::LanguageServer,
    language_server_types::ParameterLabelType,
    text_utils::{comment_highlights, search_highlights},
    theme::{Theme, THEMES},
    tree_sitter::TreeSitter,
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct RenderLayout {
    pub row_offset: usize,
    pub col_offset: usize,
    pub num_rows: usize,
    pub num_cols: usize,
}

pub struct Renderer {
    context: GraphicsContext,
    pub theme: Theme,
}

impl Renderer {
    pub fn new(window: &Window) -> Self {
        let context = GraphicsContext::new(window);

        Self {
            context,
            theme: THEMES[0],
        }
    }

    pub fn cycle_theme(&mut self) {
        let i = THEMES
            .iter()
            .position(|theme| *theme == self.theme)
            .unwrap();
        self.theme = THEMES[(i + 1) % THEMES.len()];
    }

    pub fn get_font_size(&self) -> (f64, f64) {
        (
            self.context.font_size.0 as f64,
            self.context.font_size.1 as f64,
        )
    }

    pub fn start_draw(&self) {
        self.context.begin_draw();
        self.context.clear(self.theme.background_color);
    }

    pub fn end_draw(&self) {
        self.context.end_draw();
    }

    pub fn draw_file_finder(
        &mut self,
        layout: &mut RenderLayout,
        workspace: &str,
        file_finder: &FileFinder,
    ) {
        let selected_item = file_finder.selection_index - file_finder.selection_view_offset;

        let mut longest_string = file_finder
            .files
            .iter()
            .max_by(|x, y| x.name.len().cmp(&y.name.len()))
            .map(|x| x.name.len() + 1)
            .unwrap_or(0);
        longest_string = max(longest_string, file_finder.search_string.len());

        layout.col_offset -= longest_string / 2;

        let mut selected_item_start_position = 0;
        let mut completion_string = String::default();
        for (i, item) in file_finder
            .files
            .iter()
            .enumerate()
            .skip(file_finder.selection_view_offset)
            .take(MAX_SHOWN_FILE_FINDER_ITEMS)
        {
            if i - file_finder.selection_view_offset == selected_item {
                selected_item_start_position = completion_string.len();
            }

            completion_string.push_str(item.name.as_os_str().to_str().unwrap());
            completion_string.push('\n');
        }

        let effects = [
            TextEffect {
                kind: TextEffectKind::ForegroundColor(self.theme.foreground_color),
                start: 0,
                length: completion_string.len(),
            },
            TextEffect {
                kind: TextEffectKind::ForegroundColor(self.theme.background_color),
                start: selected_item_start_position,
                length: file_finder.files[file_finder.selection_index].name.len(),
            },
        ];

        self.context.draw_completion_popup(
            0,
            0,
            layout,
            &file_finder.search_string,
            file_finder.selection_index - file_finder.selection_view_offset,
            completion_string.as_bytes(),
            self.theme.selection_background_color,
            self.theme.background_color,
            Some(&effects),
            &self.theme,
        );
    }

    pub fn draw_status_line(
        &mut self,
        workspace: &Option<String>,
        opened_file: &Option<String>,
        layout: &RenderLayout,
    ) {
        self.context.fill_cells(
            0,
            0,
            layout,
            (layout.num_cols, 2),
            self.theme.status_line_background_color,
        );

        let (status_line, effects) = if let Some(opened_file) = opened_file {
            let mut effects = vec![];
            if let Some(workspace) = workspace {
                if workspace.is_prefix_of(opened_file) {
                    effects.push(TextEffect {
                        kind: TextEffectKind::ForegroundColor(self.theme.tree_sitter_colors[3]),
                        start: 1,
                        length: workspace.len(),
                    });
                }
            }
            (format!(" {}", opened_file), effects)
        } else {
            (
                format!(
                    " {}",
                    workspace
                        .as_ref()
                        .unwrap_or(&String::from("No workspace open"))
                ),
                vec![],
            )
        };
        self.context.draw_text(
            0,
            0,
            layout,
            status_line.as_bytes(),
            &effects,
            &self.theme,
            false,
        );
    }

    pub fn draw_buffer(
        &mut self,
        buffer: &Buffer,
        layout: &RenderLayout,
        view: &View,
        language_server: &Option<Rc<RefCell<LanguageServer>>>,
        tree_sitter: &Option<Rc<RefCell<TreeSitter>>>,
    ) {
        use TextEffectKind::*;

        let text = view.visible_text(buffer, layout);
        let text_offset = view.visible_text_offset(buffer);

        let mut effects = vec![TextEffect {
            kind: ForegroundColor(self.theme.foreground_color),
            start: 0,
            length: text.len(),
        }];

        if let Some(tree_sitter) = &tree_sitter {
            effects.extend(
                tree_sitter
                    .borrow_mut()
                    .highlight_text(&text, text.len(), &self.theme),
            );
        }

        if let Some(language) = buffer.language {
            for (start, length) in comment_highlights(
                &text,
                language.line_comment_token,
                language.multi_line_comment_token_pair,
                buffer.piece_table.iter_chars_at_rev(
                    buffer
                        .piece_table
                        .char_index_from_line_col(view.line_offset, 0)
                        .unwrap_or(0),
                ),
            ) {
                effects.push(TextEffect {
                    kind: ForegroundColor(self.theme.comment_color),
                    start,
                    length,
                })
            }
        }

        if buffer.input.as_bytes().first().is_some_and(|c| *c == b'/') {
            let mut first_result_found = false;
            for (start, length) in search_highlights(&text, &buffer.input[1..]) {
                let (row, col) = (
                    view.absolute_to_view_row(buffer.piece_table.line_index(text_offset + start)),
                    view.absolute_to_view_col(buffer.piece_table.col_index(text_offset + start)),
                );

                let (foreground_color, background_color) = if !first_result_found
                    && buffer.cursors.last().is_some_and(|cursor| {
                        let ahead_of_cursor = text_offset + start >= cursor.position;
                        first_result_found = ahead_of_cursor;
                        ahead_of_cursor
                    }) {
                    (
                        self.theme.active_search_foreground_color,
                        self.theme.active_search_background_color,
                    )
                } else {
                    (
                        self.theme.search_foreground_color,
                        self.theme.search_background_color,
                    )
                };

                self.context
                    .fill_cells(row, col, layout, (length, 1), background_color);
                self.context
                    .fill_cells(row, col, layout, (1, 1), self.theme.cursor_color);
                effects.push(TextEffect {
                    kind: ForegroundColor(self.theme.background_color),
                    start,
                    length,
                });
            }
        } else {
            if buffer.mode != BufferMode::Insert {
                view.visible_cursors_iter(layout, buffer, |row, col, num| {
                    self.context.fill_cells(
                        row,
                        col,
                        layout,
                        (num, 1),
                        self.theme.selection_background_color,
                    );
                });
            }

            view.visible_cursor_leads_iter(buffer, layout, |row, col, pos| {
                if buffer.mode == BufferMode::Insert {
                    self.context
                        .fill_cell_slim_line(row, col, layout, self.theme.cursor_color);
                } else {
                    self.context
                        .fill_cells(row, col, layout, (1, 1), self.theme.cursor_color);
                    effects.push(TextEffect {
                        kind: ForegroundColor(self.theme.background_color),
                        start: pos - text_offset,
                        length: 1,
                    })
                }
            });
        }

        self.context
            .draw_text_fit_view(view, layout, &text, &effects, &self.theme);

        if let Some(server) = language_server {
            if let Some(diagnostics) = server
                .borrow()
                .saved_diagnostics
                .get(&buffer.uri.to_ascii_lowercase())
            {
                view.visible_diagnostic_lines_iter(
                    buffer,
                    layout,
                    diagnostics,
                    |row, col, count| {
                        self.context.underline_cells(
                            row,
                            col,
                            layout,
                            count,
                            self.theme.diagnostic_color,
                        );
                    },
                );
            }
        }

        view.visible_completions(buffer, layout, |completions, completion_view, request| {
            let selected_item = request.selection_index - request.selection_view_offset;

            self.context.fill_cells(
                completion_view.row,
                completion_view.col.saturating_sub(1),
                layout,
                (completion_view.width + 1, completion_view.height),
                self.theme.selection_background_color,
            );
            self.context.fill_cells(
                completion_view.row + selected_item,
                completion_view.col.saturating_sub(1),
                layout,
                (completion_view.width + 1, 1),
                self.theme.cursor_color,
            );

            let mut selected_item_start_position = 0;
            let mut completion_string = String::default();
            for (i, item) in completions
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
                    kind: ForegroundColor(self.theme.foreground_color),
                    start: 0,
                    length: completion_string.len(),
                },
                TextEffect {
                    kind: ForegroundColor(self.theme.background_color),
                    start: selected_item_start_position,
                    length: completions[request.selection_index]
                        .insert_text
                        .as_ref()
                        .unwrap_or(&completions[request.selection_index].label)
                        .len(),
                },
            ];

            self.context.draw_text(
                completion_view.row,
                completion_view.col,
                layout,
                completion_string.as_bytes(),
                &effects,
                &self.theme,
                false,
            );
        });

        view.visible_signature_helps(buffer, layout, |signature_help, signature_help_view| {
            if let Some(active_signature) = signature_help
                .signatures
                .get(signature_help.active_signature.unwrap_or(0) as usize)
            {
                let active_parameter = active_signature
                    .active_parameter
                    .or(signature_help.active_parameter);

                let mut effects = vec![];
                if let Some(parameters) = &active_signature.parameters {
                    if let Some(active_parameter) =
                        active_parameter.and_then(|i| parameters.get(i as usize))
                    {
                        match &active_parameter.label {
                            ParameterLabelType::String(label) => {
                                for (start, _) in
                                    active_signature.label.match_indices(label.as_str())
                                {
                                    if !active_signature.label.as_bytes()[start + label.len()]
                                        .is_ascii_alphanumeric()
                                    {
                                        effects.push(TextEffect {
                                            kind: ForegroundColor(
                                                self.theme.active_parameter_color,
                                            ),
                                            start,
                                            length: label.len(),
                                        });
                                    }
                                }
                            }
                            ParameterLabelType::Offsets(start, end) => {
                                effects.push(TextEffect {
                                    kind: ForegroundColor(self.theme.foreground_color),
                                    start: *start as usize,
                                    length: *end as usize - *start as usize + 1,
                                });
                            }
                        }
                    }
                }

                self.context.draw_popup_above(
                    signature_help_view.row,
                    signature_help_view.col,
                    layout,
                    active_signature.label.as_bytes(),
                    self.theme.selection_background_color,
                    self.theme.background_color,
                    Some(&effects),
                    &self.theme,
                );
            }
        });

        if let Some(server) = language_server {
            if let Some(diagnostics) = server
                .borrow()
                .saved_diagnostics
                .get(&buffer.uri.to_ascii_lowercase())
            {
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

                        let diagnostic_on_cursor_line = buffer.mode == BufferMode::Insert
                            && buffer.cursors.iter().any(|cursor| {
                                (start_line..=end_line)
                                    .contains(&buffer.piece_table.line_index(cursor.position))
                            });

                        !diagnostic_on_cursor_line
                            && ((start_line == line && (start_col..=end_col).contains(&col))
                                || (end_line == line && (start_col..=end_col).contains(&col))
                                || (diagnostic.range.start.line as usize
                                    ..diagnostic.range.end.line as usize)
                                    .contains(&line))
                    }) {
                        let (row, col) = (
                            view.absolute_to_view_row(line) + 1,
                            view.absolute_to_view_col(col),
                        );

                        self.context.draw_popup_below(
                            row,
                            col,
                            layout,
                            diagnostic.message.as_bytes(),
                            self.theme.selection_background_color,
                            self.theme.background_color,
                            None,
                            &self.theme,
                        );
                    }
                }
            }
        }

        if buffer
            .input
            .as_bytes()
            .first()
            .is_some_and(|c| *c == b':' || *c == b'/')
        {
            self.context.draw_popup_below(
                layout.num_rows,
                0,
                layout,
                buffer.input.as_bytes(),
                self.theme.selection_background_color,
                self.theme.background_color,
                None,
                &self.theme,
            );
        }
    }

    pub fn draw_numbers(&mut self, buffer: &Buffer, layout: &RenderLayout, view: &View) {
        let mut numbers = String::default();
        let num_lines = buffer.piece_table.num_lines();
        for line in view.line_offset + 1..=min(view.line_offset + 1 + layout.num_rows, num_lines) {
            numbers.push_str(line.to_string().as_str());
            numbers.push(b' ' as char);
            numbers.push(b'\n' as char);
        }

        self.context.draw_text(
            0,
            0,
            layout,
            numbers.as_bytes(),
            &[TextEffect {
                kind: TextEffectKind::ForegroundColor(self.theme.numbers_color),
                start: 0,
                length: numbers.len(),
            }],
            &self.theme,
            true,
        );
    }
}

impl Color {
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
        }
    }
}

impl RenderLayout {
    pub fn default() -> Self {
        Self {
            row_offset: 0,
            col_offset: 0,
            num_rows: 0,
            num_cols: 0,
        }
    }
}
