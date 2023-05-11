use std::{cell::RefCell, collections::HashMap, rc::Rc};

use winit::{
    dpi::LogicalPosition,
    event::{ModifiersState, VirtualKeyCode},
    window::Window,
};

use crate::{
    buffer::Buffer, language_server::LanguageServer, language_server_types::VoidParams,
    language_support::language_from_path, renderer::{Renderer, RenderLayout}, tree_sitter::TreeSitter, view::View,
};

struct Document {
    buffer: Buffer,
    view: View,
}

pub enum EditorCommand {
    CenterView,
    CenterIfNotVisible,
    Quit,
    QuitNoCheck,
}

pub struct Editor {
    renderer: Renderer,
    documents: HashMap<String, Document>,
    active_document: Option<String>,
    language_servers: HashMap<&'static str, Rc<RefCell<LanguageServer>>>,
    tree_sitters: HashMap<&'static str, Rc<RefCell<TreeSitter>>>,
}

impl Editor {
    pub fn new(window: &Window) -> Self {
        Self {
            renderer: Renderer::new(window),
            documents: HashMap::default(),
            active_document: None,
            language_servers: HashMap::default(),
            tree_sitters: HashMap::default(),
        }
    }

    pub fn handle_lsp_responses(&mut self) -> bool {
        let mut require_redraw = false;

        for (identifier, server) in &mut self.language_servers {
            let mut server = server.borrow_mut();
            match server.handle_responses() {
                Ok((responses, notifications)) => {
                    for response in responses {
                        match response.method {
                            "initialize" => {
                                for document in self.documents.values() {
                                    if *identifier == document.buffer.language.unwrap().identifier {
                                        document.buffer.send_did_open(&mut server);
                                    }
                                }
                            }
                            "textDocument/completion" => {
                                if let Some(value) = response.value {
                                    server.save_completions(response.id, value);
                                }
                                if let Some(document) = &self.active_document {
                                    self.documents
                                        .get_mut(document)
                                        .unwrap()
                                        .buffer
                                        .update_completions(&mut server);
                                }
                                require_redraw = true;
                            }
                            "textDocument/signatureHelp" => {
                                if let Some(value) = response.value {
                                    server.save_signature_help(response.id, value);
                                }
                                if let Some(document) = &self.active_document {
                                    self.documents
                                        .get_mut(document)
                                        .unwrap()
                                        .buffer
                                        .update_signature_helps(&mut server);
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
                    todo!();
                }
            }
        }

        require_redraw
    }

    pub fn render(&mut self, window: &Window) {
        self.renderer.start_draw();
        if let Some(document) = &self.active_document {
            let document = &self.documents[document];

            let window_size = (
                window.inner_size().width as f64 / window.scale_factor(),
                window.inner_size().height as f64 / window.scale_factor(),
            );

            let numbers_num_cols = (0..)
                .take_while(|i| 10usize.pow(*i) <= document.buffer.piece_table.num_lines())
                .count()
                + 2;

            let font_size = self.renderer.get_font_size();
            let buffer_layout = RenderLayout {
                row_offset: 0,
                col_offset: numbers_num_cols,
                num_rows: (window_size.1 / font_size.1).ceil() as usize,
                num_cols: (window_size.0 / font_size.0).ceil() as usize,
            };

            let numbers_layout = RenderLayout {
                row_offset: 0,
                col_offset: 0,
                num_rows: buffer_layout.num_rows,
                num_cols: numbers_num_cols,
            };

            self.renderer.draw_buffer(
                &document.buffer,
                &buffer_layout,
                &document.view,
                &document.buffer.language_server,
                &document.buffer.tree_sitter,
            );

            self.renderer.draw_numbers(&document.buffer, &numbers_layout, &document.view);
        }
        self.renderer.end_draw();
    }

    pub fn shutdown(&mut self) {
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
    ) {
        let font_size = self.renderer.get_font_size();
        if let Some(document) = self.active_document() {
            let (line, col) = document.view.get_line_col(mouse_position, font_size);
            let numbers_col_offset = (0..)
                .take_while(|i| 10usize.pow(*i) <= document.buffer.piece_table.num_lines())
                .count()
                + 2;
            if modifiers.is_some_and(|modifiers| modifiers.contains(ModifiersState::SHIFT)) {
                document.buffer.insert_cursor(line, col.saturating_sub(numbers_col_offset));
            } else {
                document.buffer.set_cursor(line, col.saturating_sub(numbers_col_offset));
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

        let font_size = self.renderer.get_font_size();
        if let Some(document) = self.active_document() {
            let (line, col) = document.view.get_line_col(mouse_position, font_size);
            let numbers_col_offset = (0..)
                .take_while(|i| 10usize.pow(*i) <= document.buffer.piece_table.num_lines())
                .count()
                + 2;
            document.buffer.set_drag(line, col.saturating_sub(numbers_col_offset));
        }
    }

    pub fn handle_mouse_double_click(
        &mut self,
        mouse_position: LogicalPosition<f64>,
        modifiers: Option<ModifiersState>,
    ) -> bool {
        let font_size = self.renderer.get_font_size();
        if let Some(document) = self.active_document() {
            let (line, col) = document.view.get_line_col(mouse_position, font_size);
            let numbers_col_offset = (0..)
                .take_while(|i| 10usize.pow(*i) <= document.buffer.piece_table.num_lines())
                .count()
                + 2;
            if modifiers.is_some_and(|modifiers| modifiers.contains(ModifiersState::SHIFT)) {
                document.buffer.insert_cursor(line, col.saturating_sub(numbers_col_offset));
            } else if document.buffer.handle_mouse_double_click(line, col.saturating_sub(numbers_col_offset)) {
                return true;
            }
        }
        false
    }

    pub fn handle_scroll(&mut self, sign: isize) {
        if let Some(document) = self.active_document() {
            document.view.handle_scroll(&document.buffer, sign);
        }
    }

    pub fn handle_mouse_hover(&mut self, mouse_position: LogicalPosition<f64>) {
        let font_size = self.renderer.get_font_size();
        if let Some(document) = self.active_document() {
            let numbers_col_offset = (0..)
                .take_while(|i| 10usize.pow(*i) <= document.buffer.piece_table.num_lines())
                .count()
                + 2;
            document.view.hover(mouse_position, font_size, numbers_col_offset);
        }
    }

    pub fn handle_mouse_exit_hover(&mut self) {
        let font_size = self.renderer.get_font_size();
        if let Some(document) = self.active_document() {
            document.view.exit_hover();
        }
    }

    pub fn hovering(&mut self) -> bool {
        if let Some(document) = self.active_document() {
            return document.view.hover.is_some();
        }
        false
    }

    pub fn has_moved_cell(
        &mut self,
        cached_mouse_position: LogicalPosition<f64>,
        mouse_position: LogicalPosition<f64>,
    ) -> bool {
        let font_size = self.renderer.get_font_size();
        if let Some(document) = self.active_document() {
            let (line, col) = document.view.get_line_col(mouse_position, font_size);
            return (line, col) != document.view.get_line_col(cached_mouse_position, font_size);
        }
        false
    }

    pub fn handle_key(
        &mut self,
        window: &Window,
        key_code: VirtualKeyCode,
        modifiers: Option<ModifiersState>,
    ) -> Option<EditorCommand> {
        if key_code == VirtualKeyCode::T
            && modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL))
        {
            self.renderer.cycle_theme();
            return None;
        }

        let font_size = self.renderer.get_font_size();
        if let Some(document) = self.active_document() {
            let window_size = (
                window.inner_size().width as f64 / window.scale_factor(),
                window.inner_size().height as f64 / window.scale_factor(),
            );

            let numbers_num_cols = (0..)
                .take_while(|i| 10usize.pow(*i) <= document.buffer.piece_table.num_lines())
                .count()
                + 2;

            let buffer_layout = RenderLayout {
                row_offset: 0,
                col_offset: numbers_num_cols,
                num_rows: (window_size.1 / font_size.1).ceil() as usize,
                num_cols: (window_size.0 / font_size.0).ceil() as usize,
            };

            if let Some(editor_command) =
                document
                    .buffer
                    .handle_key(key_code, modifiers, &document.view, buffer_layout.num_rows, buffer_layout.num_cols)
            {
                match editor_command {
                    EditorCommand::CenterView => {
                        document.view.center(&document.buffer, &buffer_layout)
                    }
                    EditorCommand::CenterIfNotVisible => {
                        document
                            .view
                            .center_if_not_visible(&document.buffer, &buffer_layout)
                    }
                    EditorCommand::Quit => {
                        return Some(EditorCommand::Quit);
                    }
                    EditorCommand::QuitNoCheck => {
                        return Some(EditorCommand::QuitNoCheck);
                    }
                }
                document.view.adjust(&document.buffer, &buffer_layout)
            }
        }
        None
    }

    pub fn handle_char(&mut self, window: &Window, c: char) -> Option<EditorCommand> {
        let font_size = self.renderer.get_font_size();
        if let Some(document) = self.active_document() {
            let window_size = (
                window.inner_size().width as f64 / window.scale_factor(),
                window.inner_size().height as f64 / window.scale_factor(),
            );

            let numbers_num_cols = (0..)
                .take_while(|i| 10usize.pow(*i) <= document.buffer.piece_table.num_lines())
                .count()
                + 2;

            let buffer_layout = RenderLayout {
                row_offset: 0,
                col_offset: numbers_num_cols,
                num_rows: (window_size.1 / font_size.1).ceil() as usize,
                num_cols: (window_size.0 / font_size.0).ceil() as usize,
            };

            if let Some(editor_command) = document.buffer.handle_char(c) {
                match editor_command {
                    EditorCommand::CenterView => {
                        document.view.center(&document.buffer, &buffer_layout)
                    }
                    EditorCommand::CenterIfNotVisible => {
                        document
                            .view
                            .center_if_not_visible(&document.buffer, &buffer_layout)
                    }
                    EditorCommand::Quit => {
                        return Some(EditorCommand::Quit);
                    }
                    EditorCommand::QuitNoCheck => {
                        return Some(EditorCommand::QuitNoCheck);
                    }
                }
            }
            document.view.adjust(&document.buffer, &buffer_layout)
            }
        None
    }

    pub fn ready_to_quit(&mut self) -> bool {
        self.documents
            .iter_mut()
            .all(|(_, document)| document.buffer.ready_to_quit())
    }

    pub fn open_file(&mut self, path: &str, window: &Window) {
        let language_server = {
            if let Some(language) = language_from_path(path) &&
                let Some(server) = LanguageServer::new(language) &&
                !self.language_servers.contains_key(language.identifier) 
                    {
                        self.language_servers
                            .insert(language.identifier, Rc::new(RefCell::new(server)));
                        Some(Rc::clone(
                            self.language_servers.get(language.identifier).unwrap(),
                        ))
                    }
            else {
                None
            }
        };

        let tree_sitter = {
            if let Some(language) = language_from_path(path) {
                if !self.tree_sitters.contains_key(language.identifier) {
                    self.tree_sitters.insert(
                        language.identifier,
                        Rc::new(RefCell::new(TreeSitter::new(language).unwrap())),
                    );
                }
                Some(Rc::clone(
                    self.tree_sitters.get(language.identifier).unwrap(),
                ))
            } else {
                None
            }
        };

        if self.documents.contains_key(path) {
            self.active_document = Some(path.to_string());
        } else {
            self.documents.insert(
                path.to_string(),
                Document {
                    buffer: Buffer::new(window, path, language_server, tree_sitter),
                    view: View::new(),
                },
            );
            self.active_document = Some(path.to_string());
        }
    }

    fn active_document(&mut self) -> Option<&mut Document> {
        if let Some(document) = &self.active_document {
            self.documents.get_mut(document)
        } else {
            None
        }
    }
}
