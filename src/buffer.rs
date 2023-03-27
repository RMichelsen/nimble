use std::{cell::RefCell, fs::File, io::BufReader, rc::Rc};

use bstr::io::BufReadExt;

use crate::{cursor::Cursor, language_support::Language, view::View};

pub enum BufferCommand {
    MoveForward(usize),
    MoveBackward(usize),
    MoveUp(usize),
    MoveDown(usize),
    MoveForwardByWord,
    MoveBackwardByWord,
    MoveToStartOfLine,
    MoveToEndOfLine,
    MoveToStartOfFile,
    MoveToEndOfFile,
    MoveForwardToChar(u8),
    MoveBackwardToChar(u8),
}

pub struct Buffer {
    pub path: String,
    pub lines: Rc<RefCell<Vec<Vec<u8>>>>,
    pub language: Rc<RefCell<Language>>,
    pub cursors: Rc<RefCell<Vec<Cursor>>>,
    pub view: View,
}

impl Buffer {
    pub fn new(path: &str, num_rows: usize, num_cols: usize) -> Self {
        let file = File::open(path).unwrap();
        let lines = Rc::new(RefCell::new(
            BufReader::new(file).byte_lines().try_collect().unwrap(),
        ));
        let language = Rc::new(RefCell::new(Language::new(path)));
        let cursors = Rc::new(RefCell::new(vec![Cursor::new(0, 0, Rc::clone(&lines))]));
        let view = View::new(
            num_rows,
            num_cols,
            Rc::clone(&lines),
            Rc::clone(&cursors),
            Rc::clone(&language),
        );

        Self {
            path: path.to_string(),
            language,
            lines,
            cursors,
            view,
        }
    }

    pub fn command(&mut self, command: BufferCommand) {
        for cursor in &mut self.cursors.borrow_mut().iter_mut() {
            match command {
                BufferCommand::MoveForward(count) => {
                    cursor.move_forward(count);
                }
                BufferCommand::MoveBackward(count) => {
                    cursor.move_backward(count);
                }
                BufferCommand::MoveUp(count) => {
                    cursor.move_up(count);
                }
                BufferCommand::MoveDown(count) => {
                    cursor.move_down(count);
                }
                BufferCommand::MoveForwardByWord => {
                    cursor.move_forward_by_word();
                }
                BufferCommand::MoveBackwardByWord => {
                    cursor.move_backward_by_word();
                }
                BufferCommand::MoveToStartOfLine => {
                    cursor.move_to_start_of_line();
                }
                BufferCommand::MoveToEndOfLine => {
                    cursor.move_to_end_of_line();
                }
                BufferCommand::MoveToStartOfFile => {
                    cursor.move_to_start_of_file();
                }
                BufferCommand::MoveToEndOfFile => {
                    cursor.move_to_end_of_file();
                }
                BufferCommand::MoveForwardToChar(c) => {
                    cursor.move_forward_to_char(c);
                }
                BufferCommand::MoveBackwardToChar(c) => {
                    cursor.move_backward_to_char(c);
                }
            }
        }
    }
}
