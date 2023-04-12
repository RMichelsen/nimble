use std::{
    borrow::BorrowMut,
    collections::{HashMap, VecDeque},
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

use bstr::ByteSlice;
use serde_json::Value;

use crate::{
    language_server_types::{
        ClientCapabilities, InitializeParams, InitializedParams, Notification, Request,
        ServerMessage, TextDocumentClientCapabilities,
    },
    language_support::Language,
};

pub struct LanguageServer {
    language: &'static Language,
    stdin: ChildStdin,
    requests: HashMap<i32, &'static str>,
    request_id: i32,
    responses: Arc<Mutex<VecDeque<ServerMessage>>>,
    initialized: bool,
    terminated: bool,
}

impl LanguageServer {
    pub fn new(language: &'static Language) -> Option<Self> {
        let mut server = Command::new(language.lsp_executable?)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .ok()?;
        let mut stdin = server.stdin.take()?;
        let responses = Arc::new(Mutex::new(VecDeque::new()));

        start_reader_thread(server, language, Arc::clone(&responses));

        send_request(
            &mut stdin,
            0,
            "initialize",
            InitializeParams {
                process_id: 0,
                root_uri: None,
                capabilities: ClientCapabilities {
                    text_document: TextDocumentClientCapabilities {},
                },
            },
        )
        .ok()?;
        let mut requests = HashMap::new();
        requests.insert(0, "initialize");

        Some(Self {
            language,
            stdin,
            requests,
            request_id: 1,
            responses,
            initialized: false,
            terminated: false,
        })
    }

    pub fn send_request<T: serde::Serialize>(&mut self, method: &'static str, params: T) {
        if self.initialized {
            match send_request(&mut self.stdin, self.request_id, method, params) {
                Ok(()) => {
                    self.requests.insert(self.request_id, method);
                    self.request_id += 1;
                }
                Err(_) => self.terminated = true,
            }
        }
    }

    pub fn send_notification<T: serde::Serialize>(&mut self, method: &'static str, params: T) {
        if self.initialized {
            match send_notification(&mut self.stdin, method, params) {
                Ok(()) => (),
                Err(_) => self.terminated = true,
            }
        }
    }

    pub fn handle_server_responses(
        &mut self,
    ) -> Result<Vec<(&'static str, Option<Value>)>, std::io::Error> {
        if self.terminated {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "Error occured, LSP must be restarted",
            ));
        }

        let mut server_responses = vec![];
        if let Ok(ref mut responses) = self.responses.try_lock() {
            while let Some(message) = responses.pop_front() {
                match message {
                    ServerMessage::Response { id, result, .. } => {
                        match self.requests.get(&id) {
                            Some(&"initialize") => {
                                send_notification(
                                    &mut self.stdin,
                                    "initialized",
                                    InitializedParams {},
                                )?;
                                self.initialized = true;
                                server_responses.push(("initialize", result));
                            }
                            Some(x) => server_responses.push((*x, result)),
                            None => (),
                        }
                        self.requests.remove(&id);
                    }
                    ServerMessage::Notification { method, params, .. } => {
                        println!("{:?}", params);
                    }
                }
            }
        }
        Ok(server_responses)
    }
}

fn start_reader_thread(
    mut server: Child,
    language: &'static Language,
    responses: Arc<Mutex<VecDeque<ServerMessage>>>,
) -> JoinHandle<()> {
    let stdout = server.stdout.take().unwrap();

    thread::spawn(move || {
        let mut buffer = vec![];
        let mut reader = BufReader::new(stdout);

        loop {
            buffer.clear();

            if let Ok(header_size) = reader.read_until(b'\n', &mut buffer) {
                if header_size > 16 {
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
                                responses.lock().unwrap().borrow_mut().push_back(message);
                                continue;
                            }
                        }
                    }
                }
            }
            break;
        }
    })
}

pub fn send_request<T: serde::Serialize>(
    stdin: &mut ChildStdin,
    request_id: i32,
    method: &'static str,
    params: T,
) -> Result<(), std::io::Error> {
    let request = Request::new(request_id, method, params);
    let message = serde_json::to_string(&request).unwrap();
    let header = format!("Content-Length: {}\r\n\r\n", message.len());
    let composed = header + message.as_str();
    stdin.write_all(composed.as_bytes())
}

fn send_notification<T: serde::Serialize>(
    stdin: &mut ChildStdin,
    method: &'static str,
    params: T,
) -> Result<(), std::io::Error> {
    let notification = Notification::new(method, params);
    let message = serde_json::to_string(&notification).unwrap();
    let header = format!("Content-Length: {}\r\n\r\n", message.len());
    let composed = header + message.as_str();
    stdin.write_all(composed.as_bytes())
}
