use std::{
    cell::RefCell,
    cmp::min,
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs::File,
    io::{BufRead, BufReader},
    rc::Rc,
};

use url::Url;
use walkdir::WalkDir;
use winit::{
    dpi::LogicalPosition,
    event::{ModifiersState, VirtualKeyCode},
    window::Window,
};

use crate::{
    buffer::Buffer,
    language_server::LanguageServer,
    language_server_types::{Hover, LocationType, VoidParams},
    language_support::{
        language_from_path, CPP_FILE_EXTENSIONS, PYTHON_FILE_EXTENSIONS, RUST_FILE_EXTENSIONS,
    },
    platform_resources,
    renderer::{RenderLayout, Renderer, POPUP_MAX_HEIGHT},
    text_utils,
    view::{HoverMessage, View, SCROLL_LINES_PER_ROLL},
};

pub const MAX_SHOWN_FILE_FINDER_ITEMS: usize = 10;

pub enum EditorCommand {
    CenterView,
    CenterIfNotVisible,
    ToggleSplitView,
    NextTab,
    PreviousTab,
    Quit,
    QuitAll,
    QuitNoCheck,
    QuitAllNoCheck,
}

struct Document {
    uri: Url,
    buffer: Buffer,
    view: View,
}

#[derive(Debug)]
pub struct FileIdentifier {
    pub name: OsString,
    pub path: OsString,
}

pub struct FileFinder {
    pub files: Vec<FileIdentifier>,
    pub search_string: String,
    pub selection_index: usize,
    pub selection_view_offset: usize,
}

pub struct Workspace {
    pub uri: Url,
    pub path: String,
    pub gitignore_paths: Vec<String>,
}

#[derive(Default, Debug)]
struct DocumentLayout {
    pub layout: RenderLayout,
    pub numbers_layout: RenderLayout,
    pub status_line_layout: RenderLayout,
}

pub struct Editor {
    renderer: Renderer,
    workspace: Option<Workspace>,
    file_finder: Option<FileFinder>,
    active_view: usize,
    split_view: bool,
    open_documents: Vec<Document>,
    visible_documents: [Vec<usize>; 2],
    visible_documents_layouts: [DocumentLayout; 2],
    file_finder_layout: RenderLayout,
    language_servers: HashMap<&'static str, Rc<RefCell<LanguageServer>>>,
}

impl Editor {
    pub fn new(window: &Window) -> Self {
        Self {
            renderer: Renderer::new(window),
            workspace: None,
            file_finder: None,
            open_documents: vec![],
            active_view: 0,
            split_view: false,
            visible_documents: [vec![], vec![]],
            visible_documents_layouts: [DocumentLayout::default(), DocumentLayout::default()],
            file_finder_layout: RenderLayout::default(),
            language_servers: HashMap::default(),
        }
    }

    pub fn update_highlights(&mut self) -> bool {
        if let Some(i) = self.visible_documents[self.active_view].last() {
            return self.open_documents[*i].buffer.update_highlights();
        }
        false
    }

    pub fn update_layouts(&mut self, window: &Window) {
        self.renderer.ensure_size(window);

        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );
        let font_size = self.renderer.get_font_size();

        self.visible_documents_layouts[0] = if let Some(i) = self.visible_documents[0].last() {
            let left_document = &mut self.open_documents[*i];
            let left_numbers_num_cols = (0..)
                .take_while(|i| 10usize.pow(*i) <= left_document.buffer.piece_table.num_lines())
                .count()
                .max(4)
                + 2;

            let left_layout = RenderLayout {
                row_offset: 0,
                col_offset: left_numbers_num_cols,
                num_rows: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(1),
                num_cols: ((window_size.0 / font_size.0 / if self.split_view { 2.0 } else { 1.0 })
                    .ceil() as usize)
                    .saturating_sub(left_numbers_num_cols),
            };

            let left_numbers_layout = RenderLayout {
                row_offset: 0,
                col_offset: 0,
                num_rows: left_layout.num_rows,
                num_cols: left_numbers_num_cols.saturating_sub(2),
            };

            let left_status_line_layout = RenderLayout {
                row_offset: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(2),
                col_offset: 0,
                num_rows: 2,
                num_cols: (window_size.0 / font_size.0 / if self.split_view { 2.0 } else { 1.0 })
                    .ceil() as usize,
            };
            DocumentLayout {
                layout: left_layout,
                numbers_layout: left_numbers_layout,
                status_line_layout: left_status_line_layout,
            }
        } else {
            DocumentLayout {
                layout: RenderLayout {
                    row_offset: 0,
                    col_offset: 0,
                    num_rows: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(1),
                    num_cols: (window_size.0
                        / font_size.0
                        / if self.split_view { 2.0 } else { 1.0 })
                    .ceil() as usize,
                },
                numbers_layout: RenderLayout::default(),
                status_line_layout: RenderLayout {
                    row_offset: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(2),
                    col_offset: 0,
                    num_rows: 2,
                    num_cols: (window_size.0
                        / font_size.0
                        / if self.split_view { 2.0 } else { 1.0 })
                    .ceil() as usize,
                },
            }
        };

        self.visible_documents_layouts[1] = if let Some(i) = self.visible_documents[1].last() {
            let right_document = &mut self.open_documents[*i];
            let right_numbers_num_cols = (0..)
                .take_while(|i| 10usize.pow(*i) <= right_document.buffer.piece_table.num_lines())
                .count()
                .max(4)
                + 2;

            let right_layout = RenderLayout {
                row_offset: 0,
                col_offset: (window_size.0 / font_size.0 / 2.0).ceil() as usize
                    + right_numbers_num_cols,
                num_rows: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(1),
                num_cols: ((window_size.0 / font_size.0 / 2.0).ceil() as usize)
                    .saturating_sub(right_numbers_num_cols),
            };

            let right_numbers_layout = RenderLayout {
                row_offset: 0,
                col_offset: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                num_rows: right_layout.num_rows,
                num_cols: right_numbers_num_cols.saturating_sub(2),
            };

            let right_status_line_layout = RenderLayout {
                row_offset: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(2),
                col_offset: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                num_rows: 2,
                num_cols: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
            };

            DocumentLayout {
                layout: right_layout,
                numbers_layout: right_numbers_layout,
                status_line_layout: right_status_line_layout,
            }
        } else {
            DocumentLayout {
                layout: RenderLayout {
                    row_offset: 0,
                    col_offset: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                    num_rows: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(1),
                    num_cols: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                },
                numbers_layout: RenderLayout::default(),
                status_line_layout: RenderLayout {
                    row_offset: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(2),
                    col_offset: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                    num_rows: 2,
                    num_cols: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                },
            }
        };

        if let (Some(workspace), Some(file_finder)) = (&self.workspace, &self.file_finder) {
            let num_cols = (window_size.0 / font_size.0).ceil() as usize;
            self.file_finder_layout = RenderLayout {
                row_offset: 0,
                col_offset: num_cols / 2,
                num_rows: (window_size.1 / font_size.1).ceil() as usize,
                num_cols,
            };
        }
    }

    pub fn open_workspace(&mut self, window: &Window) -> bool {
        if let Some(path) = platform_resources::open_folder(window) {
            self.workspace = Some(Workspace::new(&path));
            return true;
        }
        false
    }

    pub fn handle_lsp_responses(&mut self, window: &Window) -> bool {
        let mut require_redraw = false;

        let mut goto_location = None;
        for (identifier, server) in &mut self.language_servers {
            let mut server = server.borrow_mut();
            match server.handle_responses() {
                Ok((responses, notifications)) => {
                    for response in responses {
                        match response.method {
                            "initialize" => {
                                for document in &self.open_documents {
                                    if let Some(language) = document.buffer.language {
                                        if *identifier == language.identifier {
                                            document.buffer.send_did_open(&mut server);
                                        }
                                    }
                                }
                            }
                            "textDocument/completion" => {
                                if let Some(value) = response.value {
                                    server.save_completions(response.id, value);
                                }
                                if let Some(i) = self.visible_documents[self.active_view].last() {
                                    self.open_documents[*i]
                                        .buffer
                                        .update_completions(&mut server);
                                }
                                require_redraw = true;
                            }
                            "textDocument/signatureHelp" => {
                                if let Some(value) = response.value {
                                    server.save_signature_help(response.id, value);
                                }
                                if let Some(i) = self.visible_documents[self.active_view].last() {
                                    self.open_documents[*i]
                                        .buffer
                                        .update_signature_helps(&mut server);
                                }
                                require_redraw = true;
                            }
                            "textDocument/definition" | "textDocument/implementation" => {
                                if let Some(value) = response.value {
                                    if let Ok(value) = serde_json::from_value::<LocationType>(value)
                                    {
                                        match value {
                                            LocationType::Location(location) => {
                                                goto_location = Some(location);
                                            }
                                            LocationType::LocationArray(locations) => {
                                                goto_location = locations.first().cloned();
                                            }
                                        }
                                    }
                                }
                                require_redraw = true;
                            }
                            "textDocument/hover" => {
                                if let Some(value) = response.value {
                                    if let Ok(hover) = serde_json::from_value::<Hover>(value) {
                                        if let Some(i) =
                                            self.visible_documents[self.active_view].last()
                                        {
                                            match hover.contents.kind.as_str() {
                                                "plaintext" => {
                                                    let num_lines = hover
                                                        .contents
                                                        .value
                                                        .as_bytes()
                                                        .iter()
                                                        .filter(|&c| *c == b'\n')
                                                        .count();
                                                    self.open_documents[*i].view.hover_message =
                                                        Some(HoverMessage {
                                                            message: hover.contents.value,
                                                            code_block_ranges: vec![],
                                                            line_offset: 0,
                                                            num_lines,
                                                        });
                                                }
                                                "markdown" => {
                                                    let markdown = hover.contents.value;

                                                    let mut processed_markdown = String::default();

                                                    let mut code_block_ranges = vec![];
                                                    let mut offset = 0;
                                                    let mut code_block_start = None;
                                                    for line in markdown.lines() {
                                                        if line.starts_with("```") {
                                                            if let Some(start) = code_block_start {
                                                                code_block_ranges
                                                                    .push((start, offset));
                                                                code_block_start = None;
                                                            } else {
                                                                code_block_start = Some(offset);
                                                            }
                                                        } else {
                                                            processed_markdown.push_str(line);
                                                            processed_markdown.push('\n');
                                                            offset = processed_markdown.len();
                                                        }
                                                    }

                                                    let num_lines = processed_markdown
                                                        .as_bytes()
                                                        .iter()
                                                        .filter(|&c| *c == b'\n')
                                                        .count();
                                                    self.open_documents[*i].view.hover_message =
                                                        Some(HoverMessage {
                                                            message: processed_markdown,
                                                            code_block_ranges,
                                                            line_offset: 0,
                                                            num_lines,
                                                        });
                                                }
                                                _ => (),
                                            }
                                        }
                                    }
                                }
                                require_redraw = true;
                            }
                            _ => (),
                        }
                    }
                    for notification in notifications {
                        if notification.method.as_str() == "textDocument/publishDiagnostics" {
                            if let Some(value) = notification.value {
                                server.save_diagnostics(value);
                            }
                            require_redraw = true;
                        }
                    }
                }
                Err(e) => {
                    panic!();
                }
            }
        }

        if let Some(location) = goto_location {
            if let Ok(path) = Url::parse(&location.uri) {
                if let Ok(file_path) = path.to_file_path() {
                    if let Some(file_path) = file_path.to_str() {
                        self.open_file(file_path, window);
                        let active_document_layout =
                            &self.visible_documents_layouts[self.active_view];
                        if let Some(i) = self.visible_documents[self.active_view].last() {
                            let document = &mut self.open_documents[*i];
                            document.buffer.set_cursor(
                                location.range.start.line as usize,
                                location.range.start.character as usize,
                            );
                            document.view.center_if_not_visible(
                                &document.buffer,
                                &active_document_layout.layout,
                            );
                            document.buffer.update_syntect(0);
                        }
                    }
                }
            }
        }

        require_redraw
    }

    pub fn render(&mut self, window: &Window) {
        self.renderer.start_draw();

        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );
        let font_size = self.renderer.get_font_size();

        if let Some(left_document) = self.visible_documents[0].last() {
            self.renderer.draw_buffer(
                &self.open_documents[*left_document].buffer,
                &self.visible_documents_layouts[0].layout,
                &self.open_documents[*left_document].view,
                &self.open_documents[*left_document].buffer.language_server,
                self.active_view == 0,
            );

            self.renderer.draw_numbers(
                &self.open_documents[*left_document].buffer,
                &self.visible_documents_layouts[0].numbers_layout,
                &self.open_documents[*left_document].view,
            );

            self.renderer.draw_status_line(
                &self.workspace,
                Some(self.open_documents[*left_document].uri.clone()),
                &self.visible_documents_layouts[0].status_line_layout,
                self.active_view == 0,
            );
        }

        if let Some(right_document) = self.visible_documents[1].last() {
            self.renderer.draw_buffer(
                &self.open_documents[*right_document].buffer,
                &self.visible_documents_layouts[1].layout,
                &self.open_documents[*right_document].view,
                &self.open_documents[*right_document].buffer.language_server,
                self.active_view == 1,
            );

            self.renderer.draw_numbers(
                &self.open_documents[*right_document].buffer,
                &self.visible_documents_layouts[1].numbers_layout,
                &self.open_documents[*right_document].view,
            );

            self.renderer.draw_status_line(
                &self.workspace,
                Some(self.open_documents[*right_document].uri.clone()),
                &self.visible_documents_layouts[1].status_line_layout,
                self.active_view == 1,
            );
        }

        if self.split_view {
            if self.visible_documents[0].is_empty() {
                self.renderer.draw_status_line(
                    &self.workspace,
                    None,
                    &self.visible_documents_layouts[0].status_line_layout,
                    self.active_view == 0,
                );
            }
            if self.visible_documents[1].is_empty() {
                self.renderer.draw_status_line(
                    &self.workspace,
                    None,
                    &self.visible_documents_layouts[1].status_line_layout,
                    self.active_view == 1,
                );
            }
            self.renderer.draw_split(window);
        } else if self.visible_documents[0].is_empty() && self.visible_documents[1].is_empty() {
            self.renderer.draw_status_line(
                &self.workspace,
                None,
                &RenderLayout {
                    row_offset: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(2),
                    col_offset: 0,
                    num_rows: 2,
                    num_cols: (window_size.0 / font_size.0).ceil() as usize,
                },
                true,
            );
        }

        if let (Some(workspace), Some(file_finder)) = (&self.workspace, &self.file_finder) {
            self.renderer.draw_file_finder(
                &mut self.file_finder_layout,
                &workspace.path,
                file_finder,
            );
        }

        self.renderer.end_draw();
    }

    pub fn lsp_shutdown(&mut self) {
        for (identifier, server) in &mut self.language_servers {
            let mut server = server.borrow_mut();
            // According to the spec clients should wait for LSP response,
            // but we don't have time for that..
            server.send_request("shutdown", VoidParams {});
            std::thread::sleep(std::time::Duration::from_millis(10));
            server.send_notification("exit", VoidParams {});
        }
    }

    pub fn handle_mouse_pressed(
        &mut self,
        mouse_position: LogicalPosition<f64>,
        modifiers: Option<ModifiersState>,
        window: &Window,
    ) {
        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );

        if self.split_view {
            self.active_view = if mouse_position.x < window_size.0 / 2.0 {
                0
            } else {
                1
            }
        }

        let active_document_layout = &self.visible_documents_layouts[self.active_view];
        let font_size = self.renderer.get_font_size();
        if let Some(i) = self.visible_documents[self.active_view].last() {
            self.open_documents[*i].view.exit_hover();

            let (line, col) = self.open_documents[*i].view.get_line_col(
                &active_document_layout.layout,
                mouse_position,
                font_size,
            );

            if modifiers.is_some_and(|modifiers| modifiers.contains(ModifiersState::SHIFT)) {
                self.open_documents[*i].buffer.insert_cursor(line, col);
            } else {
                self.open_documents[*i].buffer.set_cursor(line, col);
            }
        }
    }

    pub fn handle_mouse_drag(
        &mut self,
        mouse_position: LogicalPosition<f64>,
        modifiers: Option<ModifiersState>,
    ) {
        if modifiers.is_some_and(|modifiers| modifiers.contains(ModifiersState::SHIFT)) {
            return;
        }
        let active_document_layout = &self.visible_documents_layouts[self.active_view];
        let font_size = self.renderer.get_font_size();
        if let Some(i) = self.visible_documents[self.active_view].last() {
            let (line, col) = self.open_documents[*i].view.get_line_col(
                &active_document_layout.layout,
                mouse_position,
                font_size,
            );
            self.open_documents[*i].buffer.set_drag(line, col);
        }
    }

    pub fn handle_mouse_double_click(
        &mut self,
        mouse_position: LogicalPosition<f64>,
        modifiers: Option<ModifiersState>,
        window: &Window,
    ) -> bool {
        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );

        if self.split_view {
            self.active_view = if mouse_position.x < window_size.0 / 2.0 {
                0
            } else {
                1
            }
        }
        let active_document_layout = &self.visible_documents_layouts[self.active_view];
        let font_size = self.renderer.get_font_size();
        if let Some(i) = self.visible_documents[self.active_view].last() {
            let (line, col) = self.open_documents[*i].view.get_line_col(
                &active_document_layout.layout,
                mouse_position,
                font_size,
            );
            if modifiers.is_some_and(|modifiers| modifiers.contains(ModifiersState::SHIFT)) {
                self.open_documents[*i].buffer.insert_cursor(line, col);
            } else if self.open_documents[*i]
                .buffer
                .handle_mouse_double_click(line, col)
            {
                return true;
            }
        }
        false
    }

    pub fn handle_scroll(
        &mut self,
        mouse_position: LogicalPosition<f64>,
        sign: isize,
        window: &Window,
    ) {
        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );

        if self.split_view {
            self.active_view = if mouse_position.x < window_size.0 / 2.0 {
                0
            } else {
                1
            }
        }

        if let Some(i) = self.visible_documents[self.active_view].last() {
            let document = &mut self.open_documents[*i];
            let old_offset = document.view.line_offset;
            document.view.handle_scroll(&document.buffer, sign);
            if document.view.line_offset != old_offset {
                document.view.exit_hover();
            }
        }
    }

    pub fn handle_mouse_hover(&mut self, mouse_position: LogicalPosition<f64>, window: &Window) {
        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );

        let active_document_layout = &self.visible_documents_layouts[self.active_view];
        let font_size = self.renderer.get_font_size();
        if let Some(i) = self.visible_documents[self.active_view].last() {
            let document = &mut self.open_documents[*i];
            document
                .view
                .hover(&active_document_layout.layout, mouse_position, font_size);

            let (line, col) = document.view.get_line_col(
                &active_document_layout.layout,
                mouse_position,
                font_size,
            );
            document.buffer.handle_mouse_hover(line, col);
        }
    }

    pub fn handle_mouse_exit_hover(&mut self) {
        let font_size = self.renderer.get_font_size();
        if let Some(i) = self.visible_documents[self.active_view].last() {
            self.open_documents[*i].view.exit_hover();
        }
    }

    pub fn hovering(&mut self) -> bool {
        if let Some(i) = self.visible_documents[self.active_view].last() {
            return self.open_documents[*i].view.hover.is_some();
        }
        false
    }

    pub fn has_moved_cell(
        &mut self,
        cached_mouse_position: LogicalPosition<f64>,
        mouse_position: LogicalPosition<f64>,
    ) -> bool {
        let active_document_layout = &self.visible_documents_layouts[self.active_view];
        let font_size = self.renderer.get_font_size();
        if let Some(i) = self.visible_documents[self.active_view].last() {
            let (line, col) = self.open_documents[*i].view.get_line_col(
                &active_document_layout.layout,
                mouse_position,
                font_size,
            );
            return (line, col)
                != self.open_documents[*i].view.get_line_col(
                    &active_document_layout.layout,
                    cached_mouse_position,
                    font_size,
                );
        }
        false
    }

    pub fn handle_key(
        &mut self,
        window: &Window,
        key_code: VirtualKeyCode,
        modifiers: Option<ModifiersState>,
    ) -> bool {
        match key_code {
            VirtualKeyCode::T if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                self.split_view = !self.split_view;
                if !self.split_view {
                    self.active_view = 0;
                }
                return true;
            }
            VirtualKeyCode::C if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                self.renderer.cycle_theme();

                for document in &mut self.open_documents {
                    document.buffer.syntect_reload(&self.renderer.theme);
                }

                return true;
            }
            VirtualKeyCode::O if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                if self.ready_to_quit() && self.open_workspace(window) {
                    self.open_documents.clear();
                    self.active_view = 0;
                    self.visible_documents[0].clear();
                    self.visible_documents[1].clear();
                    self.lsp_shutdown();
                    self.language_servers.clear();
                }

                return true;
            }
            VirtualKeyCode::P
                if self.workspace.is_some()
                    && modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) =>
            {
                self.file_finder = Some(FileFinder::new(self.workspace.as_ref().unwrap()));
                return true;
            }
            VirtualKeyCode::J if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                if let Some(file_finder) = &mut self.file_finder {
                    let num_shown_file_finder_items =
                        min(file_finder.files.len(), MAX_SHOWN_FILE_FINDER_ITEMS);
                    file_finder.selection_index = min(
                        file_finder.selection_index + 1,
                        file_finder.files.len().saturating_sub(1),
                    );
                    if file_finder.selection_index
                        >= file_finder.selection_view_offset + num_shown_file_finder_items
                    {
                        file_finder.selection_view_offset += 1;
                    }
                    return true;
                } else if let Some(i) = self.visible_documents[self.active_view].last() {
                    if let Some(hover_message) = &mut self.open_documents[*i].view.hover_message {
                        hover_message.line_offset = min(
                            hover_message.line_offset + SCROLL_LINES_PER_ROLL as usize,
                            hover_message.num_lines.saturating_sub(POPUP_MAX_HEIGHT),
                        );
                    }
                }
            }
            VirtualKeyCode::K if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                if let Some(file_finder) = &mut self.file_finder {
                    file_finder.selection_index = file_finder.selection_index.saturating_sub(1);
                    if file_finder.selection_index < file_finder.selection_view_offset {
                        file_finder.selection_view_offset -= 1;
                    }
                    return true;
                } else if let Some(i) = self.visible_documents[self.active_view].last() {
                    if let Some(hover_message) = &mut self.open_documents[*i].view.hover_message {
                        hover_message.line_offset = hover_message
                            .line_offset
                            .saturating_sub(SCROLL_LINES_PER_ROLL as usize);
                    }
                }
            }
            VirtualKeyCode::Back if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                if let Some(file_finder) = &mut self.file_finder {
                    file_finder.search_string.clear();
                    file_finder.selection_index = 0;
                    file_finder.selection_view_offset = 0;
                    return true;
                }
            }
            VirtualKeyCode::Back => {
                if let Some(file_finder) = &mut self.file_finder {
                    file_finder.search_string.pop();
                    file_finder.filter_files();
                    file_finder.selection_index = 0;
                    file_finder.selection_view_offset = 0;
                    return true;
                }
            }
            VirtualKeyCode::Return => {
                if let Some(file_finder) = &mut self.file_finder {
                    if let Some(path) = file_finder.files[file_finder.selection_index]
                        .path
                        .clone()
                        .to_str()
                    {
                        self.open_file(path, window);
                    }

                    self.file_finder = None;
                    return true;
                }
            }
            VirtualKeyCode::Escape => {
                if let Some(file_finder) = &mut self.file_finder {
                    self.file_finder = None;
                    return true;
                }
            }
            _ if self.file_finder.is_some() => return true,
            _ => (),
        }

        let active_document_layout = &self.visible_documents_layouts[self.active_view];
        let font_size = self.renderer.get_font_size();
        let mut delayed_command = None;
        if let Some(i) = self.visible_documents[self.active_view].last() {
            let document = &mut self.open_documents[*i];

            if let Some(editor_command) = document.buffer.handle_key(
                key_code,
                modifiers,
                &document.view,
                &active_document_layout.layout,
            ) {
                match editor_command {
                    EditorCommand::CenterView => document
                        .view
                        .center(&document.buffer, &active_document_layout.layout),
                    EditorCommand::CenterIfNotVisible => document
                        .view
                        .center_if_not_visible(&document.buffer, &active_document_layout.layout),
                    EditorCommand::ToggleSplitView => {
                        self.split_view = !self.split_view;
                        if !self.split_view {
                            self.active_view = 0;
                        }
                    }
                    EditorCommand::NextTab => {
                        if self.visible_documents[self.active_view].len() > 1 {
                            let front = self.visible_documents[self.active_view].remove(0);
                            self.visible_documents[self.active_view].push(front);
                        }
                    }
                    EditorCommand::PreviousTab => {
                        if self.visible_documents[self.active_view].len() > 1 {
                            let back = self.visible_documents[self.active_view].pop().unwrap();
                            self.visible_documents[self.active_view].insert(0, back);
                        }
                    }
                    x => delayed_command = Some(x),
                }
                document
                    .view
                    .adjust(&document.buffer, &active_document_layout.layout)
            }
        }

        if let Some(command) = delayed_command {
            return self.run_editor_quit_command(command);
        }

        true
    }

    pub fn handle_char(&mut self, window: &Window, c: char) -> bool {
        if let Some(file_finder) = &mut self.file_finder {
            if c as u8 >= 0x20 && c as u8 <= 0x7E {
                file_finder.search_string.push(c);
                file_finder.filter_files();
                file_finder.selection_index = 0;
                file_finder.selection_view_offset = 0;
            }
            return true;
        }

        let active_document_layout = &self.visible_documents_layouts[self.active_view];
        let font_size = self.renderer.get_font_size();
        let mut delayed_command = None;
        if let Some(i) = self.visible_documents[self.active_view].last() {
            let document = &mut self.open_documents[*i];

            if let Some(editor_command) = document.buffer.handle_char(c) {
                match editor_command {
                    EditorCommand::CenterView => document
                        .view
                        .center(&document.buffer, &active_document_layout.layout),
                    EditorCommand::CenterIfNotVisible => document
                        .view
                        .center_if_not_visible(&document.buffer, &active_document_layout.layout),
                    EditorCommand::ToggleSplitView => {
                        self.split_view = !self.split_view;
                        if !self.split_view {
                            self.active_view = 0;
                        }
                    }
                    EditorCommand::NextTab => {
                        if self.visible_documents[self.active_view].len() > 1 {
                            let front = self.visible_documents[self.active_view].remove(0);
                            self.visible_documents[self.active_view].push(front);
                        }
                    }
                    EditorCommand::PreviousTab => {
                        if self.visible_documents[self.active_view].len() > 1 {
                            let back = self.visible_documents[self.active_view].pop().unwrap();
                            self.visible_documents[self.active_view].insert(0, back);
                        }
                    }
                    x => delayed_command = Some(x),
                }
            }
            document
                .view
                .adjust(&document.buffer, &active_document_layout.layout)
        }

        if let Some(command) = delayed_command {
            return self.run_editor_quit_command(command);
        }

        true
    }

    fn run_editor_quit_command(&mut self, quit_command: EditorCommand) -> bool {
        match quit_command {
            EditorCommand::Quit => {
                let ready_to_quit = self.visible_documents[self.active_view]
                    .last()
                    .is_some_and(|i| self.open_documents[*i].buffer.ready_to_quit());

                if ready_to_quit {
                    let active_document_index =
                        *self.visible_documents[self.active_view].last().unwrap();
                    self.open_documents.remove(active_document_index);

                    if self.open_documents.is_empty() {
                        self.visible_documents[0].clear();
                        self.visible_documents[1].clear();
                    } else {
                        self.visible_documents[self.active_view].pop();
                        let documents = self.visible_documents.split_array_mut::<1>();
                        for index in documents.0[0].iter_mut().chain(documents.1[0].iter_mut()) {
                            if *index >= active_document_index {
                                *index = min(
                                    index.saturating_sub(1),
                                    self.open_documents.len().saturating_sub(1),
                                );
                            }
                        }
                    }
                }
                true
            }
            EditorCommand::QuitNoCheck => {
                let active_document_index =
                    *self.visible_documents[self.active_view].last().unwrap();
                self.open_documents.remove(active_document_index);

                if self.open_documents.is_empty() {
                    self.visible_documents[0].clear();
                    self.visible_documents[1].clear();
                } else {
                    self.visible_documents[self.active_view].pop();
                    let documents = self.visible_documents.split_array_mut::<1>();
                    for index in documents.0[0].iter_mut().chain(documents.1[0].iter_mut()) {
                        if *index >= active_document_index {
                            *index = min(
                                index.saturating_sub(1),
                                self.open_documents.len().saturating_sub(1),
                            );
                        }
                    }
                }
                true
            }
            EditorCommand::QuitAll => {
                let ready_to_quit = self.ready_to_quit();
                self.open_documents.clear();
                self.active_view = 0;
                self.visible_documents[0].clear();
                self.visible_documents[1].clear();
                false
            }
            EditorCommand::QuitAllNoCheck => {
                self.open_documents.clear();
                self.active_view = 0;
                self.visible_documents[0].clear();
                self.visible_documents[1].clear();
                false
            }
            _ => panic!(),
        }
    }

    pub fn ready_to_quit(&mut self) -> bool {
        self.open_documents
            .iter_mut()
            .all(|document| document.buffer.ready_to_quit())
    }

    pub fn open_file(&mut self, path: &str, window: &Window) {
        let language_server = language_from_path(path).map(|language| {
            if !self.language_servers.contains_key(language.identifier) {
                LanguageServer::new(language, self.workspace.as_ref().unwrap()).and_then(
                    |server| {
                        self.language_servers
                            .insert(language.identifier, Rc::new(RefCell::new(server)))
                    },
                );
            }
            Rc::clone(self.language_servers.get(language.identifier).unwrap())
        });

        let uri = Url::from_file_path(path).unwrap();

        if let Some(i) = self
            .open_documents
            .iter()
            .position(|document| document.uri == uri)
        {
            self.visible_documents[self.active_view].retain(|&x| x != i);
            self.visible_documents[self.active_view].push(i);
        } else {
            self.open_documents.push(Document {
                uri,
                buffer: Buffer::new(window, path, &self.renderer.theme, language_server),
                view: View::new(),
            });
            self.visible_documents[self.active_view]
                .push(self.open_documents.len().saturating_sub(1));

            if let Some(language) = language_from_path(path) {
                if let Some(server) = self.language_servers.get(language.identifier) {
                    let mut server = server.borrow_mut();
                    self.open_documents
                        .last_mut()
                        .unwrap()
                        .buffer
                        .send_did_open(&mut server);
                }
            }
        }
    }

    fn active_document_layout(&self) -> &DocumentLayout {
        &self.visible_documents_layouts[self.active_view]
    }
}

impl Workspace {
    pub fn new(path: &str) -> Self {
        let gitignore_paths = if let Ok(gitignore) = File::open(path.to_string() + "/.gitignore") {
            BufReader::new(gitignore)
                .lines()
                .flatten()
                .map(|entry| entry.trim_start_matches('/').to_string())
                .map(|entry| entry.trim_start_matches('\\').to_string())
                .collect()
        } else {
            vec![]
        };

        Self {
            uri: Url::from_directory_path(path).unwrap(),
            path: path.to_string(),
            gitignore_paths,
        }
    }
}

impl FileFinder {
    pub fn new(workspace: &Workspace) -> Self {
        Self {
            files: WalkDir::new(&workspace.path)
                .into_iter()
                .filter_entry(|e| {
                    e.file_name() != OsStr::new(".git")
                        && !workspace
                            .gitignore_paths
                            .iter()
                            .any(|entry| entry == e.file_name().to_str().unwrap())
                })
                .flatten()
                .filter(|e| {
                    e.file_type().is_file()
                        && e.path().extension().is_some_and(|extension| {
                            let extension = extension.to_str().unwrap();
                            CPP_FILE_EXTENSIONS.contains(&extension)
                                || RUST_FILE_EXTENSIONS.contains(&extension)
                                || PYTHON_FILE_EXTENSIONS.contains(&extension)
                                || extension == "txt"
                        })
                })
                .map(|e| FileIdentifier {
                    name: e.file_name().to_os_string(),
                    path: e.path().as_os_str().to_os_string(),
                })
                .take(1000)
                .collect(),
            search_string: String::default(),
            selection_index: 0,
            selection_view_offset: 0,
        }
    }

    pub fn filter_files(&mut self) {
        self.files.sort_by(|file1, file2| {
            if let (Some(name1), Some(name2)) = (file1.name.to_str(), file2.name.to_str()) {
                let score1 =
                    text_utils::fuzzy_match(self.search_string.as_bytes(), name1.as_bytes());
                let score2 =
                    text_utils::fuzzy_match(self.search_string.as_bytes(), name2.as_bytes());
                return score2.cmp(&score1);
            }
            0.cmp(&0)
        });
    }
}
