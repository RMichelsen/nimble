use std::{cell::RefCell, collections::HashMap, rc::Rc};

use winit::{event::VirtualKeyCode, window::Window};

use crate::{
    buffer::{Buffer, DeviceInput},
    language_server::LanguageServer,
    language_support::language_from_path,
    renderer::Renderer,
    view::View,
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

    pub fn update(&mut self) {
        if let Some(document) = &self.active_document {
            let document = &self.documents[document];
            self.renderer.draw_buffer(&document.buffer, &document.view);
        }
    }

    pub fn handle_input(&mut self, event: DeviceInput) {
        if let Some(document) = self.active_document() {
            document.view.handle_input(&document.buffer, event);
        }
    }

    pub fn handle_key(&mut self, key_code: VirtualKeyCode) {
        let (num_rows, num_cols) = (self.renderer.num_rows, self.renderer.num_cols);
        if let Some(document) = self.active_document() {
            document.buffer.handle_key(key_code);
            document.view.adjust(&document.buffer, num_rows, num_cols);
        }
    }

    pub fn handle_char(&mut self, c: char) {
        let (num_rows, num_cols) = (self.renderer.num_rows, self.renderer.num_cols);
        if let Some(document) = self.active_document() {
            document.buffer.handle_char(c);
            document.view.adjust(&document.buffer, num_rows, num_cols);
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
                    &self.language_servers.get(language.identifier).unwrap(),
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
