use std::path::Path;

pub const RUST_LINE_COMMENT_TOKEN: &str = "//";
pub const RUST_MULTI_LINE_COMMENT_TOKEN_PAIR: [&str; 2] = ["/*", "*/"];
pub const RUST_LANGUAGE_SERVER: &str = "rust-analyzer";
pub const RUST_FILE_EXTENSIONS: [&str; 1] = ["rs"];
pub const RUST_IDENTIFIER: &str = "rust";
pub const RUST_INDENT_CHARS: [u8; 3] = [b'{', b'(', b'['];

pub const CPP_LINE_COMMENT_TOKEN: &str = "//";
pub const CPP_MULTI_LINE_COMMENT_TOKEN_PAIR: [&str; 2] = ["/*", "*/"];
pub const CPP_LANGUAGE_SERVER: &str = "clangd";
pub const CPP_FILE_EXTENSIONS: [&str; 6] = ["c", "h", "cpp", "hpp", "cc", "cxx"];
pub const CPP_IDENTIFIER: &str = "cpp";
pub const CPP_INDENT_WORDS: [&str; 6] = ["if", "else", "while", "do", "for", "switch"];
pub const CPP_INDENT_CHARS: [u8; 3] = [b'{', b'(', b'['];

pub const PYTHON_LINE_COMMENT_TOKEN: &str = "#";
pub const PYTHON_FILE_EXTENSIONS: [&str; 1] = ["py"];
pub const PYTHON_IDENTIFIER: &str = "python";
pub const PYTHON_INDENT_CHARS: [u8; 1] = [b':'];

pub struct Language {
    pub identifier: &'static str,
    pub lsp_executable: Option<&'static str>,
    pub line_comment_token: Option<&'static str>,
    pub multi_line_comment_token_pair: Option<[&'static str; 2]>,
    pub indent_words: Option<&'static [&'static str]>,
    pub indent_chars: Option<&'static [u8]>,
}

pub const CPP_LANGUAGE: Language = Language {
    identifier: CPP_IDENTIFIER,
    lsp_executable: Some(CPP_LANGUAGE_SERVER),
    line_comment_token: Some(CPP_LINE_COMMENT_TOKEN),
    multi_line_comment_token_pair: Some(CPP_MULTI_LINE_COMMENT_TOKEN_PAIR),
    indent_words: Some(&CPP_INDENT_WORDS),
    indent_chars: Some(&CPP_INDENT_CHARS),
};

pub const RUST_LANGUAGE: Language = Language {
    identifier: RUST_IDENTIFIER,
    lsp_executable: Some(RUST_LANGUAGE_SERVER),
    line_comment_token: Some(RUST_LINE_COMMENT_TOKEN),
    multi_line_comment_token_pair: Some(RUST_MULTI_LINE_COMMENT_TOKEN_PAIR),
    indent_words: None,
    indent_chars: Some(&RUST_INDENT_CHARS),
};

pub const PYTHON_LANGUAGE: Language = Language {
    identifier: PYTHON_IDENTIFIER,
    lsp_executable: None,
    line_comment_token: Some(PYTHON_LINE_COMMENT_TOKEN),
    multi_line_comment_token_pair: None,
    indent_words: None,
    indent_chars: Some(&PYTHON_INDENT_CHARS),
};

pub fn language_from_path(path: &str) -> Option<&'static Language> {
    if let Some(os_str) = Path::new(path).extension() {
        if let Some(extension) = os_str.to_str() {
            if CPP_FILE_EXTENSIONS.contains(&extension) {
                return Some(&CPP_LANGUAGE);
            } else if RUST_FILE_EXTENSIONS.contains(&extension) {
                return Some(&RUST_LANGUAGE);
            } else if PYTHON_FILE_EXTENSIONS.contains(&extension) {
                return Some(&PYTHON_LANGUAGE);
            }
        }
    }
    None
}
