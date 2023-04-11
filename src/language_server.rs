use std::{
    borrow::{Borrow, BorrowMut},
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    ops::DerefMut,
    process::{ChildStdin, Command, Stdio},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

use bstr::ByteSlice;

use crate::{
    language_server_types::{
        ClientCapabilities, InitializeParams, InitializeResult, InitializedParams, Notification,
        Request, ServerMessage, TextDocumentClientCapabilities,
    },
    language_support::Language,
};

pub struct LanguageServer {
    language: &'static Language,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    requests: Arc<Mutex<HashMap<i32, &'static str>>>,
    request_id: i32,
    // responses: Arc<Mutex<VecDeque<Response>>>
}

impl LanguageServer {
    pub fn new(language: &'static Language) -> Option<Self> {
        let stdin = Arc::new(Mutex::new(None));
        let requests = Arc::new(Mutex::new(HashMap::new()));
        start_reader_thread(language, Arc::clone(&stdin), Arc::clone(&requests));

        let mut language_server = Self {
            language,
            stdin,
            requests,
            request_id: 0,
        };
        language_server.send_request(
            "initialize",
            InitializeParams {
                process_id: 0,
                root_uri: None,
                capabilities: ClientCapabilities {
                    text_document: TextDocumentClientCapabilities {},
                },
            },
        );
        language_server.send_notification("initialized", InitializedParams {});
        Some(language_server)
    }

    pub fn send_request<T: serde::Serialize>(&mut self, method: &'static str, params: T) {
        let request = Request::new(self.request_id, method, params);
        self.requests
            .lock()
            .unwrap()
            .borrow_mut()
            .insert(self.request_id, method);
        let message = serde_json::to_string(&request).unwrap();
        let header = format!("Content-Length: {}\r\n\r\n", message.len());
        let composed = header + message.as_str();
        if let Some(stdin) = self.stdin.lock().unwrap().deref_mut() {
            stdin.write_all(composed.as_bytes()).unwrap();
        }

        self.request_id += 1;
    }

    pub fn send_notification<T: serde::Serialize>(&mut self, method: &'static str, params: T) {
        let notification = Notification::new(method, params);
        let message = serde_json::to_string(&notification).unwrap();
        let header = format!("Content-Length: {}\r\n\r\n", message.len());
        let composed = header + message.as_str();
        if let Some(stdin) = self.stdin.lock().unwrap().deref_mut() {
            stdin.write_all(composed.as_bytes()).unwrap();
        }
        self.request_id += 1;
    }
}

fn start_reader_thread(
    language: &'static Language,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    requests: Arc<Mutex<HashMap<i32, &'static str>>>,
) -> JoinHandle<()> {
    // TODO: Error handling
    thread::spawn(move || {
        let mut server = Command::new(language.lsp_executable.unwrap())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let stdout = server.stdout.take().unwrap();
        *stdin.lock().unwrap() = server.stdin.take();

        let mut buffer = vec![];
        let mut reader = BufReader::new(stdout);

        loop {
            buffer.clear();

            if let Ok(header_size) = reader.read_until(b'\n', &mut buffer) {
                // TODO: This sometimes fails
                if let Ok(content_length) =
                    unsafe { std::str::from_utf8_unchecked(&buffer[16..header_size - 2]) }
                        .parse::<usize>()
                {
                    if reader.read_until(b'\n', &mut buffer).is_ok()
                        && (buffer.ends_with_str("\r\n\r\n")
                            || (reader.read_until(b'\n', &mut buffer).is_ok()
                                && buffer.ends_with_str("\r\n\r\n")))
                    {
                        let mut content = vec![0; content_length];
                        if reader.read_exact(&mut content).is_ok() {
                            let message =
                                serde_json::from_slice::<ServerMessage>(&content).unwrap();

                            match message {
                                ServerMessage::Response {
                                    jsonrpc,
                                    id,
                                    result,
                                    error,
                                } => {
                                    if let Some(result) = result {
                                        match requests.lock().unwrap().borrow().get(&id) {
                                            Some(&"initialize") => {
                                                println!(
                                                    "{:#?}",
                                                    serde_json::from_value::<InitializeResult>(
                                                        result
                                                    )
                                                    .unwrap()
                                                );
                                            }
                                            _ => (),
                                        }
                                        requests.lock().unwrap().borrow_mut().remove(&id);
                                    }
                                }
                                ServerMessage::Notification {
                                    jsonrpc,
                                    method,
                                    params,
                                } => {
                                    println!("{:?}", params);
                                }
                            }

                            continue;
                        }
                    }
                }
            }

            eprintln!("LSP Server encountered an error!");
            break;
        }
    })
}
