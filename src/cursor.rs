use std::{
    cell::RefCell,
    cmp::{max, min},
    ops::Range,
    rc::Rc,
};

use crate::{
    language_server::LanguageServer,
    language_server_types::{CompletionItem, CompletionList},
    piece_table::PieceTable,
    text_utils::{self, CharType},
};

const MAX_CURSOR_CLIPBOARD_SIZE: usize = 256;
#[derive(Copy, Clone, Debug)]
pub struct Cursor {
    pub position: usize,
    pub anchor: usize,
    pub cached_col: usize,
    pub completion_request: Option<CompletionRequest>,
    pub signature_help_request: Option<SignatureHelpRequest>,
    pub clipboard: [u8; MAX_CURSOR_CLIPBOARD_SIZE],
    pub clipboard_size: usize,
}

#[derive(Copy, Clone, Debug)]
pub struct CompletionRequest {
    pub id: i32,
    pub position: usize,
    pub selection_index: usize,
    pub selection_view_offset: usize,
}

#[derive(Copy, Clone, Debug)]
pub struct SignatureHelpRequest {
    pub id: i32,
    pub next_id: Option<i32>,
    pub position: usize,
    pub next_position: Option<usize>,
}

#[derive(Debug)]
pub struct SelectionRange {
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

pub fn cursors_overlapping(c1: &Cursor, c2: &Cursor) -> bool {
    min(c1.position, c1.anchor) <= max(c2.position, c2.anchor)
        && min(c2.position, c2.anchor) <= max(c1.position, c1.anchor)
}

pub fn cursors_delete_rebalance(cursors: &mut [Cursor], start: usize, end: usize) {
    let count = end - start;
    for cursor in cursors {
        if cursor.position > start {
            cursor.position = cursor.position.saturating_sub(count);
        }
        if cursor.anchor > start {
            cursor.anchor = cursor.anchor.saturating_sub(count);
        }
    }
}

pub fn cursors_insert_rebalance(cursors: &mut [Cursor], position: usize, count: usize) {
    for cursor in cursors {
        if cursor.position > position {
            cursor.position += count;
        }
        if cursor.anchor > position {
            cursor.anchor += count;
        }
    }
}

pub fn get_filtered_completions(
    piece_table: &PieceTable,
    completion_list: &CompletionList,
    request: &CompletionRequest,
    cursor_position: usize,
) -> Vec<CompletionItem> {
    let match_string: Vec<u8> = piece_table
        .iter_chars_at(request.position)
        .take(cursor_position - request.position)
        .collect();

    let mut filtered_completions: Vec<CompletionItem> = completion_list
        .items
        .iter()
        .filter(|item| {
            item.insert_text
                .as_ref()
                .unwrap_or(&item.label)
                .starts_with(unsafe { std::str::from_utf8_unchecked(&match_string) })
        })
        .cloned()
        .collect();

    // If the match string doesn't match anything, show all entries
    if filtered_completions.is_empty() {
        filtered_completions = completion_list.items.to_vec();
    }

    filtered_completions
}

impl Cursor {
    pub fn new(position: usize) -> Self {
        Self {
            position,
            anchor: position,
            cached_col: 0,
            completion_request: None,
            signature_help_request: None,
            clipboard: [b'\0'; MAX_CURSOR_CLIPBOARD_SIZE],
            clipboard_size: 0,
        }
    }

    pub fn range(&self) -> Range<usize> {
        min(self.position, self.anchor)..max(self.position, self.anchor)
    }

    pub fn default() -> Self {
        Self {
            position: 0,
            anchor: 0,
            cached_col: 0,
            completion_request: None,
            signature_help_request: None,
            clipboard: [b'\0'; MAX_CURSOR_CLIPBOARD_SIZE],
            clipboard_size: 0,
        }
    }

    pub fn stick_col(&mut self, piece_table: &PieceTable) {
        self.cached_col = max(self.cached_col, piece_table.col_index(self.position));
    }

    pub fn unstick_col(&mut self, piece_table: &PieceTable) {
        self.cached_col = piece_table.col_index(self.position);
    }

    pub fn move_down(&mut self, piece_table: &PieceTable, count: usize) {
        let index = piece_table.line_index(self.position);
        if let Some(line) = piece_table.line_at_index(index + 1) {
            let col = piece_table.col_index(self.position);
            self.position = line.start + min(max(col, self.cached_col), line.length);
        }
    }

    pub fn move_up(&mut self, piece_table: &PieceTable, count: usize) {
        let index = piece_table.line_index(self.position);
        if index == 0 {
            return;
        }

        if let Some(line) = piece_table.line_at_index(index - 1) {
            let col = piece_table.col_index(self.position);
            self.position = line.start + min(max(col, self.cached_col), line.length);
        }
    }

    pub fn move_forward(&mut self, piece_table: &PieceTable, count: usize) {
        let c = piece_table.char_at(self.position);
        if c.is_none() || c.is_some_and(|c| c == b'\n') {
            return;
        }

        if let Some(chars_until_newline) = self.chars_until_char(piece_table, b'\n') {
            self.position += min(count, chars_until_newline + 1);
        } else {
            self.position += count;
        }
    }

    pub fn move_forward_once_wrapping(&mut self, piece_table: &PieceTable) {
        self.position = min(self.position + 1, piece_table.num_chars().saturating_sub(1));
    }

    pub fn move_backward(&mut self, piece_table: &PieceTable, count: usize) {
        if let Some(chars_until_newline) = self.chars_until_char_rev(piece_table, b'\n') {
            self.position -= min(count, chars_until_newline);
        } else {
            self.position = self.position.saturating_sub(count);
        }
    }

    pub fn move_forward_by_word(&mut self, piece_table: &PieceTable) {
        let mut count = 0;
        for (c1, c2) in piece_table
            .iter_chars_at(self.position)
            .zip(piece_table.iter_chars_at(self.position).skip(1))
        {
            count += 1;
            let type1 = text_utils::char_type(c1);
            let type2 = text_utils::char_type(c2);

            // Special case: empty line is considered a word
            if (c1 == b'\n' && c2 == b'\n') || (type2 != CharType::Whitespace && type1 != type2) {
                self.position += count;
                return;
            }
        }
        self.position = piece_table.num_chars().saturating_sub(1);
    }

    pub fn move_backward_by_word(&mut self, piece_table: &PieceTable) {
        let mut count = 0;
        for (c1, c2) in piece_table
            .iter_chars_at_rev(self.position.saturating_sub(1))
            .zip(
                piece_table
                    .iter_chars_at_rev(self.position.saturating_sub(1))
                    .skip(1),
            )
        {
            count += 1;
            let type1 = text_utils::char_type(c1);
            let type2 = text_utils::char_type(c2);

            // Special case: empty line is considered a word
            if (c1 == b'\n' && c2 == b'\n') || (type1 != CharType::Whitespace && type1 != type2) {
                self.position -= count;
                return;
            }
        }
        self.position = 0;
    }

    pub fn move_to_start_of_line(&mut self, piece_table: &PieceTable) {
        if let Some(line) = piece_table.line_at_char(self.position) {
            self.position = line.start;
        }
    }

    pub fn move_to_end_of_line(&mut self, piece_table: &PieceTable) {
        if let Some(line) = piece_table.line_at_char(self.position) {
            self.position = line.end;
        }
    }

    pub fn move_to_start_of_file(&mut self) {
        self.position = 0;
    }

    pub fn move_to_end_of_file(&mut self, piece_table: &PieceTable) {
        self.position = piece_table.num_chars().saturating_sub(1);
    }

    pub fn move_to_char_inc(&mut self, piece_table: &PieceTable, search_char: u8) {
        if let Some(count) = self.chars_until_char(piece_table, search_char) {
            if piece_table
                .line_at_char(self.position)
                .is_some_and(|line| line.end < self.position + count + 1)
            {
                return;
            }
            self.move_forward(piece_table, count + 1);
        }
    }

    pub fn move_back_to_char_inc(&mut self, piece_table: &PieceTable, search_char: u8) {
        if let Some(count) = self.chars_until_char_rev(piece_table, search_char) {
            if piece_table
                .line_at_char(self.position)
                .is_some_and(|line| line.start > self.position.saturating_sub(count + 1))
            {
                return;
            }
            self.move_backward(piece_table, count + 1);
        }
    }

    pub fn move_to_char_exc(&mut self, piece_table: &PieceTable, search_char: u8) {
        if let Some(count) = self.chars_until_char(piece_table, search_char) {
            if piece_table
                .line_at_char(self.position)
                .is_some_and(|line| line.end < self.position + count)
            {
                return;
            }
            self.move_forward(piece_table, count);
        }
    }

    pub fn move_back_to_char_exc(&mut self, piece_table: &PieceTable, search_char: u8) {
        if let Some(count) = self.chars_until_char_rev(piece_table, search_char) {
            if piece_table
                .line_at_char(self.position)
                .is_some_and(|line| line.start > self.position.saturating_sub(count))
            {
                return;
            }
            self.move_backward(piece_table, count);
        }
    }

    pub fn move_to_first_non_blank_char(&mut self, piece_table: &PieceTable) {
        if let Some(line) = piece_table.line_at_char(self.position) {
            self.position = line.start;
            if line.length > 1 {
                if let Some(count) =
                    self.chars_until_pred(piece_table, |c| !c.is_ascii_whitespace() || c == b'\n')
                {
                    self.move_forward(
                        piece_table,
                        count
                            + piece_table
                                .char_at(line.start)
                                .unwrap()
                                .is_ascii_whitespace() as usize,
                    );
                }
            }
        }
    }

    pub fn extend_selection(&mut self, piece_table: &PieceTable) {
        if self.position == self.anchor {
            if let Some(line) = piece_table.line_at_char(self.position) {
                self.anchor = line.start;
                self.position = line.end;
            }
        } else if let Some(line) = piece_table.line_at_char(self.position) {
            if let Some(anchor_line) = piece_table.line_at_char(self.anchor) {
                if self.moving_forward() {
                    self.anchor = anchor_line.start;
                    self.position = line.end;
                } else {
                    self.anchor = anchor_line.end;
                    self.position = line.start;
                }
            }
        }
    }

    pub fn extend_selection_to_word(&mut self, piece_table: &PieceTable) {
        if let Some(line) = piece_table.line_at_char(self.position) {
            if let Some(c) = piece_table.char_at(self.position) {
                let char_type = text_utils::char_type(c);

                if let (Some(backward_match), Some(forward_match)) = (
                    (self.chars_until_pred_rev(piece_table, |c| {
                        text_utils::char_type(c) != char_type
                    })),
                    (self.chars_until_pred(piece_table, |c| text_utils::char_type(c) != char_type)),
                ) {
                    self.anchor = max(line.start, self.position - backward_match);
                    self.position = min(line.end, self.position + forward_match);
                }
            }
        }
    }

    pub fn extend_selection_to_left_word_boundary(&mut self, piece_table: &PieceTable) {
        if let Some(c) = piece_table.char_at(self.position) {
            let char_type = text_utils::char_type(c);

            if let Some(backward_match) =
                self.chars_until_pred_rev(piece_table, |c| text_utils::char_type(c) != char_type)
            {
                self.position -= backward_match;
            }
        }
    }

    pub fn extend_selection_inside(&mut self, piece_table: &PieceTable, search_char: u8) {
        let pair = match search_char {
            b'<' | b'>' => (b'<', b'>'),
            b'"' => (b'"', b'"'),
            b'\'' => (b'\'', b'\''),
            b'(' | b')' => (b'(', b')'),
            b'{' | b'}' => (b'{', b'}'),
            b'[' | b']' => (b'[', b']'),
            b'w' => return self.extend_selection_to_word(piece_table),
            _ => return,
        };

        let mut backward_count = 0;
        let mut forward_count = 0;
        if let (Some(backward_match), Some(forward_match)) = (
            self.chars_until_pred_rev(piece_table, |c| {
                if c == pair.1 {
                    backward_count += 1
                }
                if c == pair.0 {
                    if backward_count > 0 {
                        backward_count -= 1;
                    } else {
                        return true;
                    }
                }
                false
            }),
            self.chars_until_pred(piece_table, |c| {
                if c == pair.0 {
                    forward_count += 1
                }
                if c == pair.1 {
                    if forward_count > 0 {
                        forward_count -= 1;
                    } else {
                        return true;
                    }
                }
                false
            }),
        ) {
            let start = self.position - backward_match;
            let end = self.position + forward_match;

            if search_char == b'"' || search_char == b'\'' {
                let line_index = piece_table.line_index(self.position);
                if piece_table.line_index(start) != line_index
                    || piece_table.line_index(end) != line_index
                {
                    return;
                }
            }

            self.anchor = start;
            self.position = end;
        }
    }

    pub fn save_selection_to_clipboard(&mut self, piece_table: &PieceTable) {
        let start = min(self.position, self.anchor);
        let end = max(self.position, self.anchor);
        let size = min(end - start + 1, MAX_CURSOR_CLIPBOARD_SIZE);

        for (i, c) in piece_table.iter_chars_at(start).enumerate().take(size) {
            self.clipboard[i] = c;
        }
        self.clipboard_size = size;
    }

    pub fn get_selection(&mut self, piece_table: &PieceTable) -> Vec<u8> {
        let start = min(self.position, self.anchor);
        let end = max(self.position, self.anchor);
        let size = end - start + 1;
        piece_table.iter_chars_at(start).take(size).collect()
    }

    pub fn reset_completion(&mut self, language_server: &mut Option<Rc<RefCell<LanguageServer>>>) {
        if let Some(server) = &language_server {
            if let Some(request) = self.completion_request {
                server.borrow_mut().saved_completions.remove(&request.id);
            }
        }
        self.completion_request = None;
    }

    pub fn reset_completion_view(
        &mut self,
        language_server: &mut Option<Rc<RefCell<LanguageServer>>>,
    ) {
        if let Some(server) = &language_server {
            if let Some(ref mut request) = self.completion_request {
                request.selection_index = 0;
                request.selection_view_offset = 0;
            }
        }
    }

    pub fn reset_signature_help(
        &mut self,
        language_server: &mut Option<Rc<RefCell<LanguageServer>>>,
    ) {
        if let Some(server) = &language_server {
            if let Some(request) = self.signature_help_request {
                server
                    .borrow_mut()
                    .saved_signature_helps
                    .remove(&request.id);
            }
        }
        self.signature_help_request = None;
    }

    pub fn reset_anchor(&mut self) {
        self.anchor = self.position;
    }

    pub fn get_selection_ranges(&self, piece_table: &PieceTable) -> Vec<SelectionRange> {
        let line = piece_table.line_index(self.position);
        let col = piece_table.col_index(self.position);
        let anchor_line = piece_table.line_index(self.anchor);
        let anchor_col = piece_table.col_index(self.anchor);

        if line == anchor_line {
            vec![SelectionRange {
                line,
                start: min(col, anchor_col),
                end: max(col, anchor_col),
            }]
        } else {
            let (first_line, first_col, last_line, last_col) = if self.position < self.anchor {
                (line, col, anchor_line, anchor_col)
            } else {
                (anchor_line, anchor_col, line, col)
            };

            let mut ranges = vec![];
            ranges.push(SelectionRange {
                line: first_line,
                start: first_col,
                end: piece_table.line_at_index(first_line).unwrap().length,
            });

            for line in first_line + 1..last_line {
                ranges.push(SelectionRange {
                    line,
                    start: 0,
                    end: piece_table.line_at_index(line).unwrap().length,
                });
            }

            ranges.push(SelectionRange {
                line: last_line,
                start: 0,
                end: last_col,
            });
            ranges
        }
    }

    pub fn at_line_end(&self, piece_table: &PieceTable) -> bool {
        piece_table
            .line_at_char(self.position)
            .is_some_and(|line| line.end == self.position)
    }

    pub fn moving_forward(&self) -> bool {
        self.position >= self.anchor
    }

    pub fn get_line_col(&self, piece_table: &PieceTable) -> (usize, usize) {
        (
            piece_table.line_index(self.position),
            piece_table.col_index(self.position),
        )
    }

    pub fn single_selection(&self) -> bool {
        self.position == self.anchor
    }

    pub fn chars_until_pred<F>(&self, piece_table: &PieceTable, pred: F) -> Option<usize>
    where
        F: FnMut(u8) -> bool,
    {
        piece_table.iter_chars_at(self.position + 1).position(pred)
    }

    pub fn chars_until_pred_rev<F>(&self, piece_table: &PieceTable, pred: F) -> Option<usize>
    where
        F: FnMut(u8) -> bool,
    {
        piece_table
            .iter_chars_at_rev(self.position.saturating_sub(1))
            .position(pred)
    }

    pub fn chars_until_char(&self, piece_table: &PieceTable, search_char: u8) -> Option<usize> {
        self.chars_until_pred(piece_table, |c| c == search_char)
    }

    pub fn chars_until_char_rev(&self, piece_table: &PieceTable, search_char: u8) -> Option<usize> {
        self.chars_until_pred_rev(piece_table, |c| c == search_char)
    }
}
