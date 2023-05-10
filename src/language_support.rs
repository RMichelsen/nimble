use std::path::Path;

#[rustfmt::skip]
pub const RUST_KEYWORDS: [&str; 38] = [
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn",
];
pub const RUST_LINE_COMMENT_TOKEN: &str = "//";
pub const RUST_MULTI_LINE_COMMENT_TOKEN_PAIR: [&str; 2] = ["/*", "*/"];
pub const RUST_LANGUAGE_SERVER: &str = "rust-analyzer";
pub const RUST_FILE_EXTENSIONS: [&str; 1] = ["rs"];
pub const RUST_IDENTIFIER: &str = "rust";
pub const RUST_INDENT_CHARS: [u8; 3] = [b'{', b'(', b'['];

#[rustfmt::skip]
pub const CPP_KEYWORDS: [&str; 92] = [
    "alignas", "alignof", "and", "and_eq", "asm", "auto", "bitand", "bitor", "bool", "break", 
    "case", "catch", "char", "char8_t", "char16_t", "char32_t", "class", "compl", "concept", 
    "const", "consteval", "constexpr", "constinit", "const_cast", "continue", "co_await", 
    "co_return", "co_yield", "decltype", "default", "delete", "do", "double", "dynamic_cast", 
    "else", "enum", "explicit", "export", "extern", "false", "float", "for", "friend", "goto", 
    "if", "inline", "int", "long", "mutable", "namespace", "new", "noexcept", "not", "not_eq", 
    "nullptr", "operator", "or", "or_eq", "private", "protected", "public", "register", 
    "reinterpret_cast", "requires", "return", "short", "signed", "sizeof", "static", 
    "static_assert", "static_cast", "struct", "switch", "template", "this", "thread_local", 
    "throw", "true", "try", "typedef", "typeid", "typename", "union", "unsigned", "using", 
    "virtual", "void", "volatile", "wchar_t", "while", "xor", "xor_eq"
];
pub const CPP_LINE_COMMENT_TOKEN: &str = "//";
pub const CPP_MULTI_LINE_TOKEN_PAIR: [&str; 2] = ["/*", "*/"];
pub const CPP_LANGUAGE_SERVER: &str = "clangd";
pub const CPP_FILE_EXTENSIONS: [&str; 6] = ["c", "h", "cpp", "hpp", "cc", "cxx"];
pub const CPP_IDENTIFIER: &str = "cpp";
pub const CPP_INDENT_WORDS: [&str; 6] = ["if", "else", "while", "do", "for", "switch"];
pub const CPP_INDENT_CHARS: [u8; 3] = [b'{', b'(', b'['];

pub struct Language {
    pub identifier: &'static str,
    pub lsp_executable: Option<&'static str>,
    pub keywords: Option<&'static [&'static str]>,
    pub line_comment_token: Option<&'static str>,
    pub multi_line_comment_token_pair: Option<[&'static str; 2]>,
    pub indent_words: Option<&'static [&'static str]>,
    pub indent_chars: Option<&'static [u8]>,
}

pub const CPP_LANGUAGE: Language = Language {
    identifier: CPP_IDENTIFIER,
    lsp_executable: Some(CPP_LANGUAGE_SERVER),
    keywords: Some(&CPP_KEYWORDS),
    line_comment_token: Some(CPP_LINE_COMMENT_TOKEN),
    multi_line_comment_token_pair: Some(CPP_MULTI_LINE_TOKEN_PAIR),
    indent_words: Some(&CPP_INDENT_WORDS),
    indent_chars: Some(&CPP_INDENT_CHARS),
};

pub const RUST_LANGUAGE: Language = Language {
    identifier: RUST_IDENTIFIER,
    lsp_executable: Some(RUST_LANGUAGE_SERVER),
    keywords: Some(&RUST_KEYWORDS),
    line_comment_token: Some(RUST_LINE_COMMENT_TOKEN),
    multi_line_comment_token_pair: Some(RUST_MULTI_LINE_COMMENT_TOKEN_PAIR),
    indent_words: None,
    indent_chars: Some(&RUST_INDENT_CHARS),
};

pub fn language_from_path(path: &str) -> Option<&'static Language> {
    if let Some(os_str) = Path::new(path).extension() {
        if let Some(extension) = os_str.to_str() {
            if CPP_FILE_EXTENSIONS.contains(&extension) {
                return Some(&CPP_LANGUAGE);
            } else if RUST_FILE_EXTENSIONS.contains(&extension) {
                return Some(&RUST_LANGUAGE);
            }
        }
    }
    None
}
