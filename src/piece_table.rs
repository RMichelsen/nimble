use std::{
    cmp::{max, min},
    fs::File,
    io::{BufReader, Read, Write},
};

use bstr::{ByteSlice, ByteVec};

pub struct PieceTable {
    pub pieces: Vec<Piece>,
    pub indent_width: usize,
    pub dirty: bool,
    original: Vec<u8>,
    add: Vec<u8>,
}

#[derive(Debug)]
pub struct Line {
    pub index: usize,
    pub start: usize,
    pub end: usize,
    pub length: usize,
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum PieceFile {
    Original,
    Add,
}

#[derive(Debug, Clone)]
pub struct Piece {
    file: PieceFile,
    start: usize,
    length: usize,
    linebreaks: Vec<usize>,
}

impl PieceTable {
    pub fn from_file(path: &str) -> Self {
        let t = std::time::Instant::now();
        let mut original = vec![];
        let mut bytes = BufReader::new(File::open(path).unwrap()).bytes().peekable();
        let mut linebreaks = vec![];
        let mut index = 0;

        let mut indentations = [0; 9];
        let mut indent_counter = usize::MAX;
        let mut previous_indent = 0;
        let mut bytes_since_line = 0;
        while let Some(byte) = bytes.next() {
            let byte = byte.unwrap();

            // Basic but probably effective indentation guess
            if indent_counter < usize::MAX {
                if byte == b'\t' {
                    indent_counter += 4;
                } else if byte.is_ascii_whitespace() {
                    indent_counter += 1;
                } else {
                    let indent_guess = indent_counter.abs_diff(previous_indent);
                    if (2..=8).contains(&indent_guess) {
                        indentations[indent_guess] += 1;
                        previous_indent = indent_counter;
                        indent_counter = usize::MAX;
                    } else {
                        previous_indent = indent_counter;
                        indent_counter = usize::MAX;
                    }
                }
            }

            // Convert '\t' to spaces until next multiple of 4
            if byte == b'\t' {
                let num = 4 - bytes_since_line % 4;
                original.append(&mut vec![b' '; num]);
                bytes_since_line += num;
                index += num;
                continue;
            }

            // Convert '\r\n' and '\r' to '\n'
            if byte != b'\r' {
                original.push(byte);

                if byte == b'\n' {
                    linebreaks.push(index);
                    indent_counter = 0;
                    bytes_since_line = 0;
                } else {
                    bytes_since_line += 1;
                }

                index += 1;
                continue;
            }

            if bytes
                .peek()
                .is_some_and(|b| *(b.as_ref().unwrap()) != b'\n')
            {
                original.push(b'\n');
                linebreaks.push(index);
                indent_counter = 0;
                bytes_since_line = 0;
                index += 1;
            }
        }

        let indent_width = {
            if let Some((i, max_indent_count)) = indentations
                .iter()
                .enumerate()
                .max_by(|(_, c1), (_, c2)| c1.cmp(c2))
            {
                if *max_indent_count > 10 {
                    i
                } else {
                    4
                }
            } else {
                4
            }
        };

        let file_length = original.len();
        Self {
            original,
            add: vec![],
            dirty: false,
            pieces: vec![Piece {
                file: PieceFile::Original,
                start: 0,
                length: file_length,
                linebreaks,
            }],
            indent_width,
        }
    }

    pub fn save_to(&mut self, path: &str) {
        let mut file = File::create(path).unwrap();

        for piece in self.pieces.iter() {
            let buffer = if piece.file == PieceFile::Original {
                &self.original
            } else {
                &self.add
            };
            file.write_all(&buffer[piece.start..piece.start + piece.length])
                .unwrap();
        }

        self.dirty = false;
    }

    pub fn iter_lines<F>(&self, start: usize, end: usize, mut f: F)
    where
        F: FnMut(&[u8]),
    {
        let mut i = 0;
        let mut end_of_last_line = String::default();
        for piece in &self.pieces {
            let buffer = if piece.file == PieceFile::Original {
                &self.original
            } else {
                &self.add
            };

            let mut offset = piece.start;
            for linebreak in &piece.linebreaks {
                if !end_of_last_line.is_empty() {
                    end_of_last_line.push_str(unsafe {
                        std::str::from_utf8_unchecked(&buffer[offset..=offset + linebreak])
                    });

                    if (start..end).contains(&i) {
                        f(end_of_last_line.as_bytes());
                    }
                    end_of_last_line.clear();

                    i += 1;
                    if i >= end {
                        return;
                    }
                } else {
                    if (start..end).contains(&i) {
                        f(&buffer[offset..=piece.start + linebreak]);
                    }

                    i += 1;
                    if i >= end {
                        return;
                    }
                }

                offset = piece.start + linebreak + 1;
            }

            end_of_last_line.push_str(unsafe {
                std::str::from_utf8_unchecked(&buffer[offset..piece.start + piece.length])
            });
        }

        if !end_of_last_line.is_empty() {
            f(end_of_last_line.as_bytes());
        }
    }

    pub fn iter_chars(&self) -> PieceTableCharIterator {
        PieceTableCharIterator {
            piece_table: self,
            piece_index: 0,
            piece_char_index: 0,
        }
    }

    pub fn iter_chars_at(&self, position: usize) -> PieceTableCharIterator {
        let mut offset = 0;
        for (i, piece) in self.pieces.iter().enumerate() {
            if (offset..offset + piece.length).contains(&position) {
                return PieceTableCharIterator {
                    piece_table: self,
                    piece_index: i,
                    piece_char_index: position - offset,
                };
            }

            offset += piece.length;
        }

        PieceTableCharIterator {
            piece_table: self,
            piece_index: self.pieces.len(),
            piece_char_index: 0,
        }
    }

    pub fn iter_chars_at_rev(&self, position: usize) -> PieceTableCharReverseIterator {
        let mut offset = 0;
        for (i, piece) in self.pieces.iter().enumerate() {
            if (offset..offset + piece.length).contains(&position) {
                return PieceTableCharReverseIterator {
                    piece_table: self,
                    piece_index: i,
                    piece_char_index: position - offset,
                };
            }

            offset += piece.length;
        }

        PieceTableCharReverseIterator {
            piece_table: self,
            piece_index: 0,
            piece_char_index: 0,
        }
    }

    pub fn num_chars(&self) -> usize {
        self.pieces.iter().fold(0, |acc, piece| acc + piece.length)
    }

    pub fn num_lines(&self) -> usize {
        self.pieces
            .iter()
            .fold(0, |acc, piece| acc + piece.linebreaks.len())
    }

    pub fn longest_line(&self) -> usize {
        let t = std::time::Instant::now();
        let mut longest_linebreak = 0;
        let mut last_linebreak = 0;
        let mut last_piece_ending_count = 0;
        for piece in &self.pieces {
            for linebreak in &piece.linebreaks {
                longest_linebreak = max(
                    linebreak - last_linebreak + last_piece_ending_count,
                    longest_linebreak,
                );
                last_linebreak = *linebreak;
                last_piece_ending_count = 0;
            }
            last_linebreak = 0;
            last_piece_ending_count = piece.start + piece.length - last_linebreak;
        }
        longest_linebreak
    }

    pub fn insert(&mut self, position: usize, bytes: &[u8]) {
        let piece = Piece {
            file: PieceFile::Add,
            start: self.add.len(),
            length: bytes.len(),
            linebreaks: bytes
                .iter()
                .enumerate()
                .filter(|(i, &c)| c == b'\n')
                .map(|(i, c)| i)
                .collect(),
        };
        self.add.push_str(bytes);

        if self.pieces.is_empty() {
            self.pieces.insert(0, piece);
            return;
        }

        let mut current_position = 0;
        for i in 0..self.pieces.len() {
            let next_position = current_position + self.pieces[i].length;
            if (current_position + 1..next_position).contains(&position) {
                // First piece
                self.pieces[i].length = position - current_position;
                let last_piece_linebreaks = self.pieces[i]
                    .linebreaks
                    .extract_if(|i| *i >= position - current_position)
                    .map(|i| i - (position - current_position))
                    .collect();

                // Second piece
                self.pieces.insert(i + 1, piece);

                // Last piece
                self.pieces.insert(
                    i + 2,
                    Piece {
                        file: self.pieces[i].file,
                        start: self.pieces[i].start + self.pieces[i].length,
                        length: next_position - position,
                        linebreaks: last_piece_linebreaks,
                    },
                );
                break;
            }
            if current_position == position {
                self.pieces.insert(i, piece);
                break;
            }
            if next_position == position {
                self.pieces.insert(i + 1, piece);
                break;
            }

            current_position = next_position;
        }

        self.dirty = true;
    }

    pub fn delete(&mut self, start: usize, end: usize) {
        let mut current_position = 0;
        for i in 0..self.pieces.len() {
            let next_position = current_position + self.pieces[i].length;

            // Delete all pieces that are covered by [start; end]
            if start <= current_position && end >= next_position {
                self.pieces[i].length = 0;
            // Delete the end of slices where the start is in [current; next]
            } else if (current_position..next_position).contains(&start) && end >= next_position {
                self.pieces[i].length -= next_position - start;
                self.pieces[i]
                    .linebreaks
                    .retain(|i| *i < start - current_position);
            // Delete the beginning of slices where the end is in [current; next]
            } else if (current_position..=next_position).contains(&end) && start <= current_position
            {
                let delete_count = end - current_position;
                self.pieces[i].linebreaks.retain(|i| *i >= delete_count);
                for linebreak in &mut self.pieces[i].linebreaks {
                    *linebreak -= delete_count;
                }
                self.pieces[i].start += delete_count;
                self.pieces[i].length -= delete_count;
            // Split the slice into two as [start; end] is contained within [current; next]
            } else if start > current_position && end < next_position {
                self.pieces[i].length = start - current_position;

                let last_piece_linebreaks: Vec<usize> = self.pieces[i]
                    .linebreaks
                    .extract_if(|i| *i >= start - current_position)
                    .collect();

                let deleted_count = end - current_position;
                self.pieces.insert(
                    i + 1,
                    Piece {
                        file: self.pieces[i].file,
                        start: self.pieces[i].start + deleted_count,
                        length: next_position - end,
                        linebreaks: last_piece_linebreaks
                            .iter()
                            .filter_map(|i| (*i >= deleted_count).then(|| i - deleted_count))
                            .collect(),
                    },
                );
                break;
            }

            current_position = next_position;
        }

        self.pieces.retain(|piece| piece.length > 0);

        self.dirty = true;
    }

    pub fn line_at_index(&self, index: usize) -> Option<Line> {
        let mut start = 0;
        let mut offset = 0;
        let mut i = 0;
        for piece in &self.pieces {
            for linebreak in &piece.linebreaks {
                let end = offset + linebreak;
                if i == index {
                    return Some(Line {
                        index,
                        start,
                        end,
                        length: end - start,
                    });
                }
                i += 1;
                start = end + 1;
            }
            offset += piece.length;
        }

        let length = offset - start;
        if index == i && length > 0 {
            Some(Line {
                index,
                start,
                end: offset,
                length,
            })
        } else {
            None
        }
    }

    pub fn line_at_char(&self, position: usize) -> Option<Line> {
        let index = self.line_index(position);
        self.line_at_index(index)
    }

    pub fn line_index(&self, position: usize) -> usize {
        let mut offset = 0;
        let mut linebreaks = 0;
        for piece in &self.pieces {
            if (offset..offset + piece.length).contains(&position) {
                return linebreaks
                    + piece
                        .linebreaks
                        .iter()
                        .filter(|&linebreak| *linebreak < position - offset)
                        .count();
            }
            linebreaks += piece.linebreaks.len();
            offset += piece.length;
        }
        linebreaks
    }

    pub fn line_indent_width_at_char(&self, position: usize) -> usize {
        if let Some(line) = self.line_at_char(position) {
            let mut count = 0;
            for c in self.iter_chars_at(line.start).take(line.length) {
                if !c.is_ascii_whitespace() {
                    break;
                }
                count += 1;
            }
            return (count / self.indent_width) * self.indent_width;
        }
        0
    }

    pub fn line_at_char_starts_with(&self, position: usize, chars: &[u8]) -> bool {
        if let Some(line) = self.line_at_char(position) {
            let bytes: Vec<u8> = self.iter_chars_at(line.start).take(line.length).collect();
            return bytes.trim().starts_with_str(chars);
        }
        false
    }

    pub fn line_at_char_ends_with(&self, position: usize, chars: &[u8]) -> bool {
        if let Some(line) = self.line_at_char(position) {
            let bytes: Vec<u8> = self.iter_chars_at(line.start).take(line.length).collect();
            return bytes.trim().ends_with_str(chars);
        }
        false
    }

    pub fn char_index_from_line_col(&self, line: usize, col: usize) -> Option<usize> {
        if let Some(line) = self.line_at_index(line) {
            return Some(line.start + min(col, line.length));
        }
        None
    }

    pub fn col_index(&self, position: usize) -> usize {
        self.iter_chars_at_rev(position.saturating_sub(1))
            .position(|c| c == b'\n')
            .unwrap_or(position)
    }

    pub fn char_at(&self, position: usize) -> Option<u8> {
        self.iter_chars_at(position).next()
    }

    pub fn text_between_lines(&self, start_line: usize, end_line: usize) -> Vec<u8> {
        if let Some(start_of_first_line) = self.char_index_from_line_col(start_line, 0) {
            let start_of_last_line = self
                .char_index_from_line_col(end_line + 1, 0)
                .unwrap_or(self.num_chars());
            let num_chars = start_of_last_line - start_of_first_line;
            return self
                .iter_chars_at(start_of_first_line)
                .take(num_chars)
                .collect();
        }
        vec![]
    }
}

pub struct PieceTableCharIterator<'a> {
    piece_table: &'a PieceTable,
    piece_index: usize,
    piece_char_index: usize,
}

impl<'a> Iterator for PieceTableCharIterator<'a> {
    type Item = u8;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(piece) = self.piece_table.pieces.get(self.piece_index) {
            let buffer = if self.piece_table.pieces[self.piece_index].file == PieceFile::Original {
                &self.piece_table.original
            } else {
                &self.piece_table.add
            };
            let piece_start = self.piece_table.pieces[self.piece_index].start;
            let piece_length = self.piece_table.pieces[self.piece_index].length;
            if self.piece_char_index < piece_length {
                let c = Some(buffer[piece_start + self.piece_char_index]);
                self.piece_char_index += 1;
                return c;
            }
            self.piece_char_index = 0;
            self.piece_index += 1;
            self.next()
        } else {
            None
        }
    }
}

pub struct PieceTableCharReverseIterator<'a> {
    piece_table: &'a PieceTable,
    piece_index: usize,
    piece_char_index: usize,
}

impl<'a> Iterator for PieceTableCharReverseIterator<'a> {
    type Item = u8;
    fn next(&mut self) -> Option<Self::Item> {
        self.piece_table
            .pieces
            .get(self.piece_index)
            .and_then(|piece| {
                let buffer = if piece.file == PieceFile::Original {
                    &self.piece_table.original
                } else {
                    &self.piece_table.add
                };

                if self.piece_char_index != usize::MAX {
                    let c = buffer.get(piece.start + self.piece_char_index).copied();
                    self.piece_char_index = self.piece_char_index.wrapping_sub(1);
                    return c;
                }

                if self.piece_index > 0 {
                    self.piece_index -= 1;
                    self.piece_char_index = self.piece_table.pieces[self.piece_index].length - 1;
                    self.next()
                } else {
                    None
                }
            })
    }
}
