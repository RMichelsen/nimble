use bstr::ByteSlice;

pub fn search_highlights(text: &[u8], match_text: &str) -> Vec<(usize, usize)> {
    if match_text.is_empty() {
        return vec![];
    }

    let mut matches = vec![];

    let mut search_index = 0;
    let mut current_text = text;
    while let Some(i) = current_text.as_bstr().find(match_text) {
        search_index += i;

        // Match here
        matches.push((search_index, match_text.len()));
        search_index += match_text.len();

        current_text = &text[search_index..];
    }

    matches
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CharType {
    Word,
    Punctuation,
    Whitespace,
}

pub fn char_type(c: u8) -> CharType {
    match c {
        c if c.is_ascii_alphanumeric() || c == b'_' => CharType::Word,
        c if c.is_ascii_whitespace() => CharType::Whitespace,
        _ => CharType::Punctuation,
    }
}

pub fn is_closing_bracket(c: u8) -> bool {
    c == b')' || c == b'}' || c == b']' || c == b'>'
}

pub fn matching_bracket(c: u8) -> u8 {
    match c {
        b'(' => b')',
        b'{' => b'}',
        b'[' => b']',
        b'<' => b'>',
        b')' => b'(',
        b'}' => b'{',
        b']' => b'[',
        b'>' => b'<',
        _ => panic!(),
    }
}
