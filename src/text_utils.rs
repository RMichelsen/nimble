pub fn find_keywords_iter<F>(line: &[u8], keywords: &[&str], mut f: F)
where
    F: FnMut(usize, usize),
{
    let mut word = String::new();
    for (i, c) in line.iter().enumerate() {
        if char_type(*c) != CharType::Word {
            if keywords.contains(&word.as_str()) {
                let len = word.len();
                f(i - len, len);
            }
            word.clear();
        } else {
            word.push(*c as char);
        }
    }

    if keywords.contains(&word.as_str()) {
        let len = word.len();
        f(line.len() - len, len);
    }
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
