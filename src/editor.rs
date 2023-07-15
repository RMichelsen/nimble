use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs::File,
    io::{BufRead, BufReader},
    os::windows::fs::FileTypeExt,
    path::{Path, PathBuf},
    rc::Rc,
};

use imgui_winit_support::winit::window::Window;
use url::Url;
use walkdir::WalkDir;

use crate::{
    buffer::Buffer,
    language_server::LanguageServer,
    language_server_types::{Hover, VoidParams},
    language_support::language_from_path,
    platform_resources, text_utils,
    theme::Theme,
    user_interface::RenderData,
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

pub struct Document {
    pub buffer: Buffer,
    uri: Url,
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

pub enum FileTreeEntry {
    File(PathBuf),
    Folder(PathBuf, Vec<FileTreeEntry>),
}

pub struct Workspace {
    pub uri: Url,
    pub path: String,
    pub gitignore_paths: Vec<String>,
    pub file_tree: Vec<FileTreeEntry>,
}

pub struct Editor {
    pub buffers: HashMap<Url, Buffer>,
    pub workspace: Option<Workspace>,
    file_finder: Option<FileFinder>,
    language_servers: HashMap<&'static str, Rc<RefCell<LanguageServer>>>,
}

impl Editor {
    pub fn new(window: &Window) -> Self {
        Self {
            workspace: None,
            file_finder: None,
            buffers: HashMap::new(),
            language_servers: HashMap::default(),
        }
    }

    pub fn update_highlights(&mut self, render_data: &RenderData) {
        for buffer in render_data.buffers.iter() {
            self.buffers.get_mut(buffer).unwrap().update_highlights();
        }
    }

    pub fn open_workspace(&mut self, window: &Window) -> bool {
        if let Some(path) = platform_resources::open_folder(window) {
            self.workspace = Some(Workspace::new(&path));
            return true;
        }
        false
    }

    pub fn handle_lsp_responses(&mut self) {
        for (identifier, server) in &mut self.language_servers {
            let mut server = server.borrow_mut();
            match server.handle_responses() {
                Some((responses, notifications)) => {
                    for response in responses {
                        match response.method {
                            "initialize" => {
                                for buffer in self.buffers.values_mut() {
                                    if let Some(language) = buffer.language {
                                        if *identifier == language.identifier {
                                            buffer.send_did_open(&mut server);
                                        }
                                    }
                                }
                            }
                            "textDocument/completion" => {
                                if let Some(value) = response.value {
                                    server.save_completions(response.id, value);
                                }
                                for buffer in self.buffers.values_mut() {
                                    buffer.update_completions(&mut server);
                                }
                            }
                            "textDocument/signatureHelp" => {
                                if let Some(value) = response.value {
                                    server.save_signature_help(response.id, value);
                                }
                                for buffer in self.buffers.values_mut() {
                                    buffer.update_signature_helps(&mut server);
                                }
                            }
                            "textDocument/definition" | "textDocument/implementation" => {
                                // TODO
                            }
                            "textDocument/hover" => {
                                if let Some(value) = response.value {
                                    server.save_hover(response.id, value);
                                }
                            }
                            _ => (),
                        }
                    }
                    for notification in notifications {
                        if notification.method.as_str() == "textDocument/publishDiagnostics" {
                            if let Some(value) = notification.value {
                                server.save_diagnostics(value);
                            }
                        }
                    }
                }
                None => panic!(),
            }
        }
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

    pub fn open_file_prompt(&mut self, window: &Window, theme: &Theme) -> Option<Url> {
        if let Some(path) = platform_resources::open_file(window) {
            return self.open_file(window, theme, &path);
        }
        None
    }

    pub fn open_file(&mut self, window: &Window, theme: &Theme, path: &str) -> Option<Url> {
        if let Ok(uri) = Url::from_file_path(path) {
            if !self.buffers.contains_key(&uri) {
                let language_server = if self.workspace.is_some() {
                    language_from_path(path).map(|language| {
                        if !self.language_servers.contains_key(language.identifier) {
                            LanguageServer::new(language, self.workspace.as_ref().unwrap())
                                .and_then(|server| {
                                    self.language_servers
                                        .insert(language.identifier, Rc::new(RefCell::new(server)))
                                });
                        }
                        Rc::clone(self.language_servers.get(language.identifier).unwrap())
                    })
                } else {
                    None
                };
                self.buffers.insert(
                    uri.clone(),
                    Buffer::new(window, &uri, theme, language_server),
                );

                if let Some(language) = language_from_path(uri.path()) {
                    if let Some(server) = self.language_servers.get(language.identifier) {
                        let mut server = server.borrow_mut();
                        self.buffers
                            .values_mut()
                            .last()
                            .unwrap()
                            .send_did_open(&mut server);
                    }
                }
            }
            return Some(uri);
        }
        None
    }

    pub fn close_file(&mut self, uri: &Url) {
        self.buffers.remove(uri);
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

        fn walk_folder(path: &Path) -> Vec<FileTreeEntry> {
            let mut file_tree = vec![];
            for entry in WalkDir::new(path)
                .sort_by_file_name()
                .max_depth(1)
                .into_iter()
                .flatten()
            {
                if entry.path() == path {
                    continue;
                }

                if entry.file_type().is_file() || entry.file_type().is_symlink_file() {
                    file_tree.push(FileTreeEntry::File(entry.path().to_owned()));
                } else if entry.file_type().is_dir() || entry.file_type().is_symlink_dir() {
                    file_tree.push(FileTreeEntry::Folder(
                        entry.path().to_owned(),
                        walk_folder(entry.path()),
                    ));
                }
            }
            file_tree.sort_by(|x, y| {
                let x_is_dir = matches!(x, FileTreeEntry::Folder(_, _)) as usize;
                let y_is_dir = matches!(y, FileTreeEntry::Folder(_, _)) as usize;
                y_is_dir.cmp(&x_is_dir)
            });
            file_tree
        }

        Self {
            uri: Url::from_directory_path(path).unwrap(),
            path: path.to_string(),
            gitignore_paths,
            file_tree: walk_folder(Path::new(path)),
        }
    }
}

impl FileFinder {
    pub fn new(workspace: &Workspace) -> Self {
        let files: Vec<FileIdentifier> = WalkDir::new(&workspace.path)
            .into_iter()
            .filter_entry(|e| {
                e.file_name() != OsStr::new(".git")
                    && !workspace
                        .gitignore_paths
                        .iter()
                        .any(|entry| entry == e.file_name().to_str().unwrap())
            })
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| FileIdentifier {
                name: e.file_name().to_os_string(),
                path: e.path().as_os_str().to_os_string(),
            })
            .take(1000)
            .collect();

        Self {
            files,
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
