use std::{cell::RefCell, collections::HashMap, rc::Rc};

use winit::{
    dpi::LogicalPosition,
    event::{ModifiersState, VirtualKeyCode},
    window::Window,
};

use crate::{
    buffer::Buffer, language_server::LanguageServer, language_server_types::VoidParams,
    language_support::language_from_path, renderer::Renderer, view::View,
};

struct Document {
    buffer: Buffer,
    view: View,
}

pub struct Editor {
    renderer: Renderer,
    documents: HashMap<String, Document>,
    active_document: Option<String>,
    language_servers: HashMap<&'static str, Rc<RefCell<LanguageServer>>>,
}

impl Editor {
    pub fn new(window: &Window) -> Self {
        Self {
            renderer: Renderer::new(window),
            documents: HashMap::default(),
            active_document: None,
            language_servers: HashMap::default(),
        }
    }

    pub fn update(&mut self) -> bool {
        let mut require_redraw = false;

        for (identifier, server) in &mut self.language_servers {
            let mut server = server.borrow_mut();
            match server.handle_server_responses() {
                Ok((responses, notifications)) => {
                    for response in responses {
                        match response.method {
                            "initialize" => {
                                for document in self.documents.values() {
                                    if *identifier == document.buffer.language.identifier {
                                        document.buffer.send_did_open(&mut server);
                                    }
                                }
                            }
                            "textDocument/completion" => {
                                if let Some(value) = response.value {
                                    server.save_completions(response.id, value);
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
                        match notification.method.as_str() {
                            "textDocument/publishDiagnostics" => {
                                if let Some(value) = notification.value {
                                    server.save_diagnostics(value);
                                }
                                require_redraw = true;
                            }
                            _ => (),
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

    pub fn render(&mut self) {
        self.renderer.start_draw();
        if let Some(document) = &self.active_document {
            let document = &self.documents[document];
            self.renderer.draw_buffer(
                &document.buffer,
                &document.view,
                &document.buffer.language_server,
            );
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
            if modifiers.is_some_and(|modifiers| modifiers.contains(ModifiersState::SHIFT)) {
                document.buffer.insert_cursor(line, col);
            } else {
                document.buffer.set_cursor(line, col);
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
            document.buffer.set_drag(line, col);
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
            if modifiers.is_some_and(|modifiers| modifiers.contains(ModifiersState::SHIFT)) {
                document.buffer.insert_cursor(line, col);
            } else {
                if document.buffer.handle_mouse_double_click(line, col) {
                    return true;
                }
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
            document.view.hover(mouse_position, font_size);
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

    pub fn handle_key(&mut self, key_code: VirtualKeyCode, modifiers: Option<ModifiersState>) {
        let (num_rows, num_cols) = (self.renderer.num_rows, self.renderer.num_cols);
        if let Some(document) = self.active_document() {
            if document
                .buffer
                .handle_key(key_code, modifiers, &document.view, num_rows, num_cols)
            {
                document.view.adjust(&document.buffer, num_rows, num_cols);
            }
        }
    }

    pub fn handle_char(&mut self, c: char) {
        let (num_rows, num_cols) = (self.renderer.num_rows, self.renderer.num_cols);
        if let Some(document) = self.active_document() {
            if document.buffer.handle_char(c) {
                document.view.adjust(&document.buffer, num_rows, num_cols);
            }
        }
    }

    pub fn open_file(&mut self, path: &str) {
        let language_server = {
            if let Some(language) = language_from_path(path) {
                if !self.language_servers.contains_key(language.identifier) {
                    self.language_servers.insert(
                        language.identifier,
                        Rc::new(RefCell::new(LanguageServer::new(language).unwrap())),
                    );
                }
                Some(Rc::clone(
                    self.language_servers.get(language.identifier).unwrap(),
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
                    buffer: Buffer::new(path, language_server),
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
