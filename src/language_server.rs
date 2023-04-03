use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use crate::language_support::Language;

pub struct LanguageServer {
    language: Language,
    server: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
}

impl LanguageServer {
    pub fn new(path: &str) -> Option<Self> {
        let language = Language::new(path);

        let mut server = Command::new(language.lsp_executable?)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .ok()?;
        let stdin = server.stdin.take()?;
        let stdout = server.stdout.take()?;

        Some(Self {
            language,
            server,
            stdin,
            stdout,
        })
    }
}
