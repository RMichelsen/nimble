use std::{
    cell::RefCell,
    cmp::{max, min},
    rc::Rc,
    str::pattern::Pattern,
};

use url::Url;
use winit::window::Window;

use crate::{
    buffer::{Buffer, BufferMode},
    editor::{FileFinder, Workspace, MAX_SHOWN_FILE_FINDER_ITEMS},
    graphics_context::GraphicsContext,
    language_server::LanguageServer,
    language_server_types::ParameterLabelType,
    text_utils::search_highlights,
    theme::{Theme, THEMES},
    view::View,
};

#[derive(Clone, Copy, Debug)]
pub enum TextEffectKind {
    ForegroundColor(Color),
}

#[derive(Clone, Copy, Debug)]
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
    pub r_u8: u8,
    pub g_u8: u8,
    pub b_u8: u8,
}

#[derive(Clone, Copy, Debug, Default)]
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

    pub fn ensure_size(&mut self, window: &Window) {
        self.context.ensure_size(window);
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
        workspace_path: &str,
        file_finder: &FileFinder,
    ) {
        if file_finder.files.is_empty() {
            return;
        }

        let selected_item = file_finder.selection_index - file_finder.selection_view_offset;

        let mut longest_string = file_finder
            .files
            .iter()
            .max_by(|x, y| x.name.len().cmp(&y.name.len()))
            .map(|x| x.name.len() + 1)
            .unwrap_or(0);
        longest_string = max(longest_string, file_finder.search_string.len());

        layout.col_offset = layout.col_offset.saturating_sub(longest_string / 2);

        let num_shown_file_finder_items = min(file_finder.files.len(), MAX_SHOWN_FILE_FINDER_ITEMS);

        let mut selected_item_start_position = 0;
        let mut completion_string = String::default();
        for (i, item) in file_finder
            .files
            .iter()
            .enumerate()
            .skip(file_finder.selection_view_offset)
            .take(num_shown_file_finder_items)
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
        workspace: &Option<Workspace>,
        opened_file: Option<Url>,
        layout: &RenderLayout,
        active: bool,
    ) {
        self.context.fill_cells(
            0,
            0,
            layout,
            (layout.num_cols, 2),
            self.theme.status_line_background_color,
        );

        let color = if active {
            self.theme.palette.fg0
        } else {
            self.theme.palette.bg2
        };

        let (status_line, mut effects) = if let Some(opened_file) = opened_file {
            let file_path = opened_file.to_file_path().unwrap();
            let mut effects = vec![];
            if let Some(workspace) = workspace {
                if workspace.path.is_prefix_of(file_path.to_str().unwrap()) {
                    effects.push(TextEffect {
                        kind: TextEffectKind::ForegroundColor(color),
                        start: 1,
                        length: workspace.path.len(),
                    });
                }
            }
            (format!(" {}", file_path.to_str().unwrap()), effects)
        } else {
            (
                format!(
                    " {}",
                    if workspace.is_some() {
                        &workspace.as_ref().unwrap().path
                    } else {
                        "No workspace open"
                    }
                ),
                vec![],
            )
        };

        effects.insert(
            0,
            TextEffect {
                kind: TextEffectKind::ForegroundColor(color),
                start: 0,
                length: status_line.len(),
            },
        );

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
        active: bool,
    ) {
        use TextEffectKind::*;

        let text = view.visible_text(buffer, layout);
        let text_offset = view.visible_text_offset(buffer);

        let mut effects = vec![TextEffect {
            kind: ForegroundColor(self.theme.foreground_color),
            start: 0,
            length: text.len(),
        }];

        if let Some(syntect) = &buffer.syntect {
            effects.extend(syntect.highlight_lines(
                &buffer.piece_table,
                view.line_offset,
                view.line_offset + layout.num_rows,
            ))
        }

        if buffer.input.as_bytes().first() == Some(&b'/') {
            let mut first_result_found = false;
            for (start, length) in search_highlights(&text, &buffer.input[1..]) {
                let (row, col) = (
                    view.absolute_to_view_row(buffer.piece_table.line_index(text_offset + start)),
                    view.absolute_to_view_col(buffer.piece_table.col_index(text_offset + start)),
                );

                let (mut foreground_color, mut background_color) = (
                    self.theme.search_foreground_color,
                    self.theme.search_background_color,
                );

                if !first_result_found
                    && buffer
                        .cursors
                        .last()
                        .is_some_and(|cursor| text_offset + start >= cursor.position)
                {
                    foreground_color = self.theme.active_search_foreground_color;
                    background_color = self.theme.active_search_background_color;
                    first_result_found = true;
                }

                self.context
                    .fill_cells(row, col, layout, (length, 1), background_color);
                self.context
                    .fill_cells(row, col, layout, (1, 1), self.theme.cursor_color);
                effects.push(TextEffect {
                    kind: ForegroundColor(foreground_color),
                    start,
                    length,
                });
            }
        } else if active {
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
                .get(&buffer.uri.to_lowercase())
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
            if completions.is_empty() {
                return;
            }

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
                .skip(request.selection_view_offset)
                .enumerate()
                .take(completion_view.height)
            {
                if i == selected_item {
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
                        .len()
                        + 1,
                },
            ];

            let detail_string = completions[request.selection_index]
                .detail
                .clone()
                .unwrap_or_default();

            let label_string = if completions[request.selection_index]
                .insert_text
                .as_ref()
                .is_some_and(|text| {
                    text.trim() != completions[request.selection_index].label.trim()
                }) {
                completions[request.selection_index].label.clone()
            } else {
                String::default()
            };

            let mut label_detail_combined = String::default();

            let longest_detail_string = detail_string
                .split('\n')
                .max_by(|x, y| x.len().cmp(&y.len()))
                .unwrap_or("")
                .len();

            if detail_string
                .as_bytes()
                .iter()
                .filter(|&c| *c == b'\n')
                .count()
                == label_string
                    .as_bytes()
                    .iter()
                    .filter(|&c| *c == b'\n')
                    .count()
            {
                for (detail, label) in detail_string.split('\n').zip(label_string.split('\n')) {
                    label_detail_combined.push_str(detail);
                    for _ in 0..longest_detail_string - detail.len() {
                        label_detail_combined.push(' ');
                    }
                    label_detail_combined.push_str(label);
                    label_detail_combined.push('\n');
                }
            }

            if !label_detail_combined.trim().is_empty() {
                let mut bytes = vec![];
                for c in label_detail_combined.as_bytes() {
                    if c.is_ascii() {
                        bytes.push(*c)
                    } else if bytes.last().is_some_and(|c| *c != b' ') {
                        bytes.push(b' ');
                    }
                }

                self.context.draw_popup_below(
                    completion_view.row,
                    completion_view.col + completion_view.width,
                    layout,
                    bytes.trim_ascii_end(),
                    self.theme.selection_background_color,
                    self.theme.background_color,
                    None,
                    &self.theme,
                    false,
                );
            }

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
                    false,
                );
            }
        });

        if buffer
            .input
            .as_bytes()
            .first()
            .is_some_and(|c| *c == b':' || *c == b'/')
        {
            self.context.draw_popup_above(
                layout.num_rows,
                0,
                layout,
                buffer.input.as_bytes(),
                self.theme.selection_background_color,
                self.theme.background_color,
                None,
                &self.theme,
                false,
            );
        }
    }
    
    pub fn draw_buffer_hovers(
        &mut self,
        buffer: &Buffer,
        layout: &RenderLayout,
        view: &View,
        language_server: &Option<Rc<RefCell<LanguageServer>>>,
    ) {
        if let Some(server) = language_server {
            if let Some(diagnostics) = server
                .borrow()
                .saved_diagnostics
                .get(&buffer.uri.to_lowercase())
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
                            view.absolute_to_view_col(col) + 1,
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
                            true,
                        );
                    } else if let Some(hover_message) = &view.hover_message {
                        println!("{}",hover_message.message);
                        // TODO: Rendering the hover message this way is pretty inefficient.
                        // However, most hovers are not many thousands characters long..
                        let (row, col) = (
                            view.absolute_to_view_row(line) + 1,
                            view.absolute_to_view_col(col) + 1,
                        );

                        let mut leading_lines = 0;
                        let mut line_limit = 0;
                        let mut offset = 0;
                        let truncated_message: Vec<u8> = hover_message
                            .message
                            .as_bytes()
                            .iter()
                            .skip_while(|&x| {
                                let skip = leading_lines < hover_message.line_offset;
                                if *x == b'\n' {
                                    leading_lines += 1;
                                }
                                offset += 1;
                                skip
                            })
                            .take_while(|&x| {
                                let limit_reached = line_limit < (layout.num_rows / 2);
                                if *x == b'\n' {
                                    line_limit += 1;
                                }
                                limit_reached
                            })
                            .copied()
                            .collect();

                        let mut offset_ranges = vec![];
                        for range in &hover_message.code_block_ranges {
                            offset_ranges.push((
                                range.0.saturating_sub(offset),
                                range.1.saturating_sub(offset),
                            ));
                        }

                        let mut effects = vec![];
                        if let Some(syntect) = &buffer.syntect {
                            effects =
                                syntect.highlight_code_blocks(&truncated_message, &offset_ranges);
                        }

                        self.context.draw_popup_below(
                            row,
                            col,
                            layout,
                            &truncated_message,
                            self.theme.selection_background_color,
                            self.theme.background_color,
                            Some(&effects),
                            &self.theme,
                            true,
                        );
                    }
                }
            }
        }
    }

    pub fn draw_numbers(&mut self, buffer: &Buffer, layout: &RenderLayout, view: &View) {
        let mut numbers = String::default();
        let num_lines = buffer.piece_table.num_lines();
        for line in view.line_offset + 1..=min(view.line_offset + 1 + layout.num_rows, num_lines) {
            numbers.push_str(line.to_string().as_str());
            numbers.push(b'\n' as char);
        }

        self.context.fill_cells(
            0,
            0,
            layout,
            (layout.num_cols + 2, layout.num_rows),
            self.theme.background_color,
        );
        self.context.draw_text(
            0,
            1,
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

    pub fn draw_split(&mut self, window: &Window) {
        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );

        let font_size = self.get_font_size();

        let num_rows = ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(1);

        let layout = RenderLayout {
            row_offset: 0,
            col_offset: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
            num_rows,
            num_cols: 2,
        };

        for i in 0..num_rows {
            self.context
                .fill_cell_slim_line(i, 0, &layout, self.theme.numbers_color);
        }
    }
}

impl Color {
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            r_u8: r,
            g_u8: g,
            b_u8: b,
        }
    }
}
