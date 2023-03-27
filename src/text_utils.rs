pub fn find_keywords_iter<F>(line: &[u8], keywords: &[&str], f: F)
where
    F: Fn(usize, usize),
{
    let mut word = String::new();
    for (i, c) in line.iter().enumerate() {
        if c.is_ascii_whitespace() {
            if keywords.contains(&word.as_str()) {
                let len = word.len();
                f(i - len, len);
            }
            word.clear();
        } else {
            word.push(*c as char);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CharType {
    Word,
    Punctuation,
    Whitespace,
}

pub fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == 0x5F
}

pub fn get_ascii_char_type(c: u8) -> CharType {
    match c {
        x if is_word(x) => CharType::Word,
        x if x.is_ascii_whitespace() => CharType::Whitespace,
        _ => CharType::Punctuation,
    }
}
