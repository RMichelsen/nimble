use std::{
    borrow::BorrowMut,
    collections::{HashMap, VecDeque},
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    mem::size_of,
    os::windows::{
        prelude::{FromRawHandle, OwnedHandle},
        process::CommandExt,
    },
    process::{Command, Stdio},
    ptr::null_mut,
    sync::{
        mpsc::{channel, Receiver, SendError, Sender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

use bstr::ByteSlice;
use serde_json::Value;
use windows::Win32::{
    Foundation::HANDLE,
    Security::SECURITY_ATTRIBUTES,
    System::{Pipes::CreatePipe, Threading::CREATE_NO_WINDOW},
};

use crate::{
    editor::Workspace,
    language_server_types::{
        ClientCapabilities, CompletionList, Diagnostic, GeneralClientCapabilities, Hover,
        HoverClientCapabilities, InitializeParams, InitializeResult, InitializedParams,
        MarkdownClientCapabilities, Notification, PublishDiagnosticParams, Request, ServerMessage,
        SignatureHelp, TextDocumentClientCapabilities,
    },
    language_support::Language,
};

pub struct ServerResponse {
    pub method: &'static str,
    pub id: i32,
    pub value: Option<Value>,
}

pub struct ServerNotification {
    pub method: String,
    pub value: Option<Value>,
}

pub struct LanguageServer {
    language: &'static Language,
    sender: Sender<String>,
    requests: HashMap<i32, &'static str>,
    request_id: i32,
    responses: Arc<Mutex<VecDeque<ServerMessage>>>,
    initialized: bool,
    terminated: bool,
    pub saved_completions: HashMap<i32, CompletionList>,
    pub saved_signature_helps: HashMap<i32, SignatureHelp>,
    pub saved_hover_messages: HashMap<i32, Hover>,
    pub saved_diagnostics: HashMap<String, Vec<Diagnostic>>,
    pub trigger_characters: Vec<u8>,
    pub signature_help_trigger_characters: Vec<u8>,
}

impl LanguageServer {
    pub fn new(language: &'static Language, workspace: &Workspace) -> Option<Self> {
        let (process_id, stdin, stdout) = if cfg!(target_os = "windows") {
            let mut stdin_read = HANDLE::default();
            let mut stdin_write = HANDLE::default();
            let mut stdout_read = HANDLE::default();
            let mut stdout_write = HANDLE::default();

            let security_attributes = SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                bInheritHandle: true.into(),
                lpSecurityDescriptor: null_mut(),
            };

            unsafe {
                CreatePipe(
                    &mut stdin_read,
                    &mut stdin_write,
                    Some(&security_attributes),
                    0,
                );
                CreatePipe(
                    &mut stdout_read,
                    &mut stdout_write,
                    Some(&security_attributes),
                    0,
                );

                let process = Command::new(language.lsp_executable?)
                    .stdin(Stdio::from_raw_handle(stdin_read.0 as *mut _))
                    .stdout(Stdio::from_raw_handle(stdout_write.0 as *mut _))
                    .stderr(Stdio::null())
                    .creation_flags(CREATE_NO_WINDOW.0)
                    .spawn()
                    .ok()?;
                (
                    process.id(),
                    File::from_raw_handle(stdin_write.0 as *mut _),
                    File::from_raw_handle(stdout_read.0 as *mut _),
                )
            }
        } else {
            let mut process = Command::new(language.lsp_executable?)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .ok()?;
            (
                process.id(),
                File::from(OwnedHandle::from(process.stdin.take()?)),
                File::from(OwnedHandle::from(process.stdout.take()?)),
            )
        };

        let responses = Arc::new(Mutex::new(VecDeque::new()));

        let (mut sender, receiver) = channel();
        start_reader_thread(stdout, language, Arc::clone(&responses));
        start_writer_thread(stdin, receiver);

        send_request(
            &mut sender,
            0,
            "initialize",
            InitializeParams {
                process_id,
                root_uri: Some(workspace.uri.to_string()),
                capabilities: ClientCapabilities {
                    general: GeneralClientCapabilities {
                        position_encodings: vec!["utf-8".to_string()],
                        markdown: MarkdownClientCapabilities {
                            parser: String::from("Python-Markdown"),
                            version: String::from("3.2.2"),
                        },
                    },
                    text_document: TextDocumentClientCapabilities {
                        hover: HoverClientCapabilities {
                            content_format: vec![
                                String::from("markdown"),
                                String::from("plaintext"),
                            ],
                        },
                    },
                },
            },
        )
        .ok()?;
        let mut requests = HashMap::new();
        requests.insert(0, "initialize");

        Some(Self {
            language,
            sender,
            requests,
            request_id: 1,
            responses,
            initialized: false,
            terminated: false,
            saved_completions: HashMap::new(),
            saved_signature_helps: HashMap::new(),
            saved_hover_messages: HashMap::new(),
            saved_diagnostics: HashMap::new(),
            trigger_characters: Vec::new(),
            signature_help_trigger_characters: Vec::new(),
        })
    }

    pub fn save_diagnostics(&mut self, value: serde_json::Value) {
        let params = serde_json::from_value::<PublishDiagnosticParams>(value).unwrap();
        self.saved_diagnostics
            .insert(params.uri.to_lowercase(), params.diagnostics);
    }

    pub fn save_completions(&mut self, request_id: i32, value: serde_json::Value) {
        self.saved_completions.insert(
            request_id,
            serde_json::from_value::<CompletionList>(value).unwrap(),
        );
    }

    pub fn save_hover(&mut self, request_id: i32, value: serde_json::Value) {
        let hover = serde_json::from_value::<Hover>(value).unwrap();
        self.saved_hover_messages.insert(request_id, hover);
    }

    pub fn save_signature_help(&mut self, request_id: i32, value: serde_json::Value) {
        let signature_help = serde_json::from_value::<SignatureHelp>(value).unwrap();
        self.saved_signature_helps
            .insert(request_id, signature_help);
    }

    pub fn send_request<T: serde::Serialize>(
        &mut self,
        method: &'static str,
        params: T,
    ) -> Option<i32> {
        if self.initialized {
            match send_request(&mut self.sender, self.request_id, method, params) {
                Ok(()) => {
                    self.requests.insert(self.request_id, method);
                    self.request_id += 1;
                    return Some(self.request_id - 1);
                }
                Err(_) => self.terminated = true,
            }
        }
        None
    }

    pub fn send_notification<T: serde::Serialize>(&mut self, method: &'static str, params: T) {
        if self.initialized {
            match send_notification(&mut self.sender, method, params) {
                Ok(()) => (),
                Err(_) => self.terminated = true,
            }
        }
    }

    pub fn handle_responses(&mut self) -> Option<(Vec<ServerResponse>, Vec<ServerNotification>)> {
        if self.terminated {
            return None;
        }

        let mut server_responses = vec![];
        let mut server_notifications = vec![];
        if let Ok(ref mut responses) = self.responses.try_lock() {
            while let Some(message) = responses.pop_front() {
                match message {
                    ServerMessage::Response { id, result, .. } => {
                        match self.requests.get(&id) {
                            Some(&"initialize") => {
                                send_notification(
                                    &mut self.sender,
                                    "initialized",
                                    InitializedParams {},
                                )
                                .ok()?;

                                if let Some(result) = result.clone() {
                                    if let Ok(result) =
                                        serde_json::from_value::<InitializeResult>(result)
                                    {
                                        if let Some(completion_provider) =
                                            result.capabilities.completion_provider
                                        {
                                            if let Some(trigger_characters) =
                                                completion_provider.trigger_characters
                                            {
                                                for c in trigger_characters {
                                                    self.trigger_characters.push(c.as_bytes()[0]);
                                                }
                                            }
                                        }

                                        if let Some(signature_help_provider) =
                                            result.capabilities.signature_help_provider
                                        {
                                            if let Some(trigger_characters) =
                                                signature_help_provider.trigger_characters
                                            {
                                                for c in trigger_characters {
                                                    self.signature_help_trigger_characters
                                                        .push(c.as_bytes()[0]);
                                                    self.trigger_characters.push(c.as_bytes()[0])
                                                }
                                            }
                                        }
                                    }
                                }

                                self.initialized = true;
                                server_responses.push(ServerResponse {
                                    method: "initialize",
                                    id,
                                    value: result,
                                });
                            }
                            Some(x) => server_responses.push(ServerResponse {
                                method: x,
                                id,
                                value: result,
                            }),
                            None => (),
                        }
                        self.requests.remove(&id);
                    }
                    ServerMessage::Notification { method, params, .. } => server_notifications
                        .push(ServerNotification {
                            method,
                            value: params,
                        }),
                }
            }
        }
        Some((server_responses, server_notifications))
    }
}

fn start_writer_thread(mut stdin: File, receiver: Receiver<String>) -> JoinHandle<()> {
    thread::spawn(move || loop {
        let message = receiver.recv().unwrap();
        match stdin.write_all(message.as_bytes()) {
            Ok(()) => (),
            _ => break,
        }
    })
}

fn start_reader_thread(
    stdout: File,
    language: &'static Language,
    responses: Arc<Mutex<VecDeque<ServerMessage>>>,
) -> JoinHandle<()> {
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

fn send_request<T: serde::Serialize>(
    sender: &mut Sender<String>,
    request_id: i32,
    method: &'static str,
    params: T,
) -> Result<(), SendError<String>> {
    let request = Request::new(request_id, method, params);
    let message = serde_json::to_string(&request).unwrap();
    let header = format!("Content-Length: {}\r\n\r\n", message.len());
    let composed = header + message.as_str();
    sender.send(composed)
}

fn send_notification<T: serde::Serialize>(
    sender: &mut Sender<String>,
    method: &'static str,
    params: T,
) -> Result<(), SendError<String>> {
    let notification = Notification::new(method, params);
    let message = serde_json::to_string(&notification).unwrap();
    let header = format!("Content-Length: {}\r\n\r\n", message.len());
    let composed = header + message.as_str();
    sender.send(composed)
}
