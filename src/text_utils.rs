use std::str::pattern::Pattern;

use bstr::ByteSlice;

use crate::piece_table::PieceTableCharReverseIterator;

pub fn find_keywords_iter<F>(text: &[u8], keywords: Option<&[&str]>, mut f: F)
where
    F: FnMut(usize, usize),
{
    if let Some(keywords) = keywords {
        let mut word = String::new();
        for (i, c) in text.iter().enumerate() {
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
            f(text.len() - len, len);
        }
    }
}

pub fn leading_multi_line_comment_end(
    text: &[u8],
    multi_line_comment_token_pair: Option<[&str; 2]>,
) -> Option<usize> {
    multi_line_comment_token_pair.and_then(|[t1, t2]| {
        text.find(t2).and_then(|i| {
            if i < text.find(t1).unwrap_or(usize::MAX) {
                Some(i)
            } else {
                None
            }
        })
    })
}

pub fn find_comments_iter<F>(
    text: &[u8],
    line_comment_token: Option<&str>,
    multi_line_comment_token_pair: Option<[&str; 2]>,
    start_iterator_rev: PieceTableCharReverseIterator,
    mut f: F,
) where
    F: FnMut(usize, usize),
{
    if let Some([t1, t2]) = multi_line_comment_token_pair {
        if let Some(leading_multi_line_comment_end) =
            leading_multi_line_comment_end(&text, multi_line_comment_token_pair)
        {
            let mut match_t1 = String::default();
            let mut match_t2 = String::default();
            for c in start_iterator_rev {
                match_t1.insert(0, c as char);
                match_t2.insert(0, c as char);
                if match_t1 == t1 {
                    f(0, leading_multi_line_comment_end + t2.len());
                    break;
                }

                if match_t2 == t2 {
                    break;
                }

                if !match_t1.is_suffix_of(t1) {
                    match_t1.clear();
                }

                if !match_t2.is_suffix_of(t2) {
                    match_t2.clear();
                }
            }
        }

        let mut slice = text;
        let mut offset = 0;
        while let Some(start) = slice.find(t1) {
            let text_start = offset + start;
            offset += start + t1.len();
            slice.take(..start + t1.len()).unwrap();
            if let Some(end) = slice.find(t2) {
                offset += end + t2.len();
                slice.take(..end + t2.len()).unwrap();
                f(text_start, end + t2.len() + 2);
            } else {
                f(text_start, text.len() - text_start);
                break;
            }
        }
    }

    if let Some(line_comment_token) = line_comment_token {
        let mut slice = text;
        let mut offset = 0;
        while let Some(start) = slice.find(line_comment_token) {
            let text_start = offset + start;
            offset += start + line_comment_token.len();
            slice.take(..start + line_comment_token.len()).unwrap();
            if let Some(end) = slice.find_byte(b'\n') {
                offset += end + 1;
                slice.take(..=end).unwrap();
                f(text_start, end + 2);
            } else {
                f(text_start, text.len() - text_start);
                break;
            }
        }
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
