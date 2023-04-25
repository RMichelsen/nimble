use std::str::pattern::Pattern;

use bstr::ByteSlice;

use crate::{
    piece_table::PieceTableCharReverseIterator,
    renderer::{TextEffect, TextEffectKind},
    theme::{COMMENT_COLOR, KEYWORD_COLOR},
};

pub fn keyword_highlights(text: &[u8], keywords: Option<&[&str]>) -> Vec<TextEffect> {
    let mut effects = vec![];

    if let Some(keywords) = keywords {
        let mut word = String::new();
        for (i, c) in text.iter().enumerate() {
            if char_type(*c) != CharType::Word {
                if keywords.contains(&word.as_str()) {
                    let length = word.len();
                    effects.push(TextEffect {
                        kind: TextEffectKind::ForegroundColor(KEYWORD_COLOR),
                        start: i - length,
                        length,
                    });
                }
                word.clear();
            } else {
                word.push(*c as char);
            }
        }

        if keywords.contains(&word.as_str()) {
            let length = word.len();
            effects.push(TextEffect {
                kind: TextEffectKind::ForegroundColor(KEYWORD_COLOR),
                start: text.len() - length,
                length,
            });
        }
    }

    effects
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

pub fn comment_highlights(
    text: &[u8],
    line_comment_token: Option<&str>,
    multi_line_comment_token_pair: Option<[&str; 2]>,
    start_iterator_rev: PieceTableCharReverseIterator,
) -> Vec<TextEffect> {
    let mut effects = vec![];

    if let Some([t1, t2]) = multi_line_comment_token_pair {
        if let Some(leading_multi_line_comment_end) =
            leading_multi_line_comment_end(text, multi_line_comment_token_pair)
        {
            let mut match_t1 = String::default();
            let mut match_t2 = String::default();
            for c in start_iterator_rev {
                match_t1.insert(0, c as char);
                match_t2.insert(0, c as char);
                if match_t1 == t1 {
                    effects.push(TextEffect {
                        kind: TextEffectKind::ForegroundColor(COMMENT_COLOR),
                        start: 0,
                        length: leading_multi_line_comment_end + t2.len(),
                    });
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
                effects.push(TextEffect {
                    kind: TextEffectKind::ForegroundColor(COMMENT_COLOR),
                    start: text_start,
                    length: end + t2.len() + 2,
                });
            } else {
                effects.push(TextEffect {
                    kind: TextEffectKind::ForegroundColor(COMMENT_COLOR),
                    start: text_start,
                    length: text.len() - text_start,
                });
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
                effects.push(TextEffect {
                    kind: TextEffectKind::ForegroundColor(COMMENT_COLOR),
                    start: text_start,
                    length: end + 2,
                });
            } else {
                effects.push(TextEffect {
                    kind: TextEffectKind::ForegroundColor(COMMENT_COLOR),
                    start: text_start,
                    length: text.len() - text_start,
                });
                break;
            }
        }
    }

    effects
}

pub fn string_highlights(text: &[u8]) -> Vec<TextEffect> {
    let mut effects = vec![];

    for token in &[b'\'', b'"'] {
        let mut slice = text;
        let mut offset = 0;
        while let Some(start) = slice.find_byte(*token) {
            let text_start = offset + start;
            offset += start + 1;
            slice.take(..start + 1).unwrap();
            if let Some(end) = slice.find_byte(*token) {
                if slice
                    .find_byte(b'\n')
                    .is_some_and(|line_end| line_end < end)
                {
                    continue;
                }

                offset += end + 1;
                slice.take(..=end).unwrap();
                effects.push(TextEffect {
                    kind: TextEffectKind::ForegroundColor(KEYWORD_COLOR),
                    start: text_start,
                    length: end + 2,
                });
            }
        }
    }

    effects
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
