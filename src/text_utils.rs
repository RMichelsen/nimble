use std::cmp::max;

use bstr::ByteSlice;

const UNMATCHED_LETTER_PENALTY: isize = -1;
const LEADING_LETTER_PENALTY: isize = -5;
const MAX_LEADING_LETTER_PENALTY: isize = -15;
const ADJACENCY_BONUS: isize = 15;
const FIRST_LETTER_BONUS: isize = 15;
const UPPERCASE_START_BONUS: isize = 30;
const SEPARATOR_BONUS: isize = 30;

fn match_score(chars_since_match: usize, c: u8, prev_c: Option<u8>) -> isize {
    match (prev_c, c) {
        (Some(prev_c), c) => {
            ADJACENCY_BONUS * (chars_since_match == 0) as isize
                + UPPERCASE_START_BONUS
                    * (c.is_ascii_uppercase() && prev_c.is_ascii_lowercase()) as isize
                + SEPARATOR_BONUS
                    * (c.is_ascii_alphanumeric() && !prev_c.is_ascii_alphanumeric()) as isize
        }
        (None, c) => FIRST_LETTER_BONUS * (chars_since_match == 0) as isize,
    }
}

fn match_recursively(pattern: &[u8], text: &[u8], prev_c: Option<u8>, score: isize) -> isize {
    if pattern.is_empty() {
        return score;
    }

    let mut sub_string = text;
    let mut best_score = isize::MIN;

    let mut chars_since_match = 0;
    while let Some(i) = sub_string
        .iter()
        .position(|&c| c.to_ascii_lowercase() == pattern[0].to_ascii_lowercase())
    {
        chars_since_match += i;
        let sub_score = match_recursively(
            &pattern[1..],
            &sub_string[i + 1..],
            Some(sub_string[i]),
            match_score(
                chars_since_match,
                sub_string[i],
                if chars_since_match == 0 || prev_c.is_none() {
                    prev_c
                } else {
                    text.get(chars_since_match - 1).copied()
                },
            ),
        );
        best_score = max(best_score, sub_score);
        sub_string = &sub_string[i + 1..];
    }

    if best_score == isize::MIN {
        isize::MIN
    } else {
        score + best_score
    }
}

// Based on https://github.com/tajmone/fuzzy-search/blob/master/fts_fuzzy_match/0.2.0/c/fts_fuzzy_match.c
pub fn fuzzy_match(pattern: &[u8], text: &[u8]) -> isize {
    let unmatched_penalty = -1;
    let mut score = 100;

    if pattern.is_empty() {
        return score;
    }
    if text.len() < pattern.len() {
        return isize::MIN;
    }

    score += UNMATCHED_LETTER_PENALTY * (text.len() - pattern.len()) as isize;

    for i in 0..pattern.len() {
        match (text.get(i), pattern.get(i)) {
            (Some(c1), Some(c2)) if c1 == c2 => (),
            _ => score += LEADING_LETTER_PENALTY,
        }
    }

    match_recursively(pattern, text, None, score)
}

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
        b')' => b'(',
        b'}' => b'{',
        b']' => b'[',
        _ => panic!(),
    }
}
