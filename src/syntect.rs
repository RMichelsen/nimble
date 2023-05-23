use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    str::FromStr,
    sync::{Arc, Mutex, RwLock},
    thread,
    time::Duration,
};

use syntect::{
    dumps::from_uncompressed_data,
    highlighting::{
        Color, HighlightState, Highlighter, RangedHighlightIterator, ScopeSelectors, StyleModifier,
        Theme, ThemeItem,
    },
    parsing::{ParseState, ScopeStack, SyntaxSet},
};

use crate::{
    piece_table::PieceTable,
    renderer::{TextEffect, TextEffectKind},
};

impl From<crate::renderer::Color> for Color {
    fn from(color: crate::renderer::Color) -> Self {
        Self {
            r: color.r_u8,
            g: color.g_u8,
            b: color.b_u8,
            a: 255,
        }
    }
}

pub const SYNTECT_CACHE_FREQUENCY: usize = 50;

pub struct IndexedLine {
    pub index: usize,
    pub text: Vec<u8>,
}

pub struct Syntect {
    pub queue: Arc<Mutex<VecDeque<IndexedLine>>>,
    pub cache_updated: Arc<Mutex<bool>>,
    cache: Arc<RwLock<HashMap<usize, Vec<TextEffect>>>>,
}

impl Syntect {
    pub fn new(path: &str, theme: &crate::theme::Theme) -> Option<Self> {
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let cache_updated = Arc::new(Mutex::new(false));
        let cache = Arc::new(RwLock::new(HashMap::new()));

        let theme = convert_theme(theme);

        start_highlight_thread(
            path,
            theme,
            Arc::clone(&queue),
            Arc::clone(&cache_updated),
            Arc::clone(&cache),
        )?;

        Some(Self {
            queue,
            cache_updated,
            cache,
        })
    }

    pub fn delete_rebalance(&mut self, piece_table: &PieceTable, position: usize, end: usize) {
        let start_index = piece_table.line_index(position) / SYNTECT_CACHE_FREQUENCY;
        let start_cache_offset = piece_table
            .char_index_from_line_col(start_index * SYNTECT_CACHE_FREQUENCY, 0)
            .unwrap();
        let start_effects_offset = position - start_cache_offset;
        if let Ok(ref mut cache) = self.cache.as_ref().write() {
            if let Some(effects) = cache.get_mut(&start_index) {
                for effect in effects {
                    if effect.start >= start_effects_offset + (end - position) {
                        effect.start = effect.start.saturating_sub(end - position);
                    }
                }
            }
        }
    }

    pub fn insert_rebalance(&mut self, piece_table: &PieceTable, position: usize, count: usize) {
        let start_index = piece_table.line_index(position) / SYNTECT_CACHE_FREQUENCY;
        let start_cache_offset = piece_table
            .char_index_from_line_col(start_index * SYNTECT_CACHE_FREQUENCY, 0)
            .unwrap();
        let start_effects_offset = position - start_cache_offset;
        if let Ok(ref mut cache) = self.cache.as_ref().write() {
            if let Some(effects) = cache.get_mut(&start_index) {
                for effect in effects {
                    if effect.start >= start_effects_offset {
                        effect.start += count;
                    }
                }
            }
        }
    }

    pub fn highlight_lines(
        &self,
        piece_table: &PieceTable,
        start: usize,
        end: usize,
    ) -> Vec<TextEffect> {
        let start_index = start / SYNTECT_CACHE_FREQUENCY;
        let start_cache_offset = piece_table
            .char_index_from_line_col(start_index * SYNTECT_CACHE_FREQUENCY, 0)
            .unwrap();
        let start_text_offset = piece_table.char_index_from_line_col(start, 0).unwrap();
        let start_effects_offset = start_text_offset - start_cache_offset;
        let mut effects = self
            .cache
            .try_read()
            .map(|cache| cache.get(&start_index).cloned())
            .unwrap_or(None)
            .unwrap_or(vec![]);

        effects.retain(|effect| effect.start >= start_effects_offset);
        for effect in &mut effects {
            effect.start -= start_effects_offset;
        }

        let end_index = end / SYNTECT_CACHE_FREQUENCY;
        if end_index != start_index {
            let end_cache_offset = piece_table
                .char_index_from_line_col(end_index * SYNTECT_CACHE_FREQUENCY, 0)
                .unwrap_or(piece_table.num_chars());
            let end_text_offset = piece_table
                .char_index_from_line_col(end, 0)
                .unwrap_or(piece_table.num_chars());
            let end_effects_offset = end_text_offset - end_cache_offset;
            let mut end_effects = self
                .cache
                .try_read()
                .map(|cache| cache.get(&end_index).cloned())
                .unwrap_or(None)
                .unwrap_or(vec![]);
            end_effects.retain(|effect| effect.start < end_effects_offset);
            for effect in &mut end_effects {
                effect.start += (end_text_offset - start_text_offset) - end_effects_offset;
            }
            effects.append(&mut end_effects);
        }

        effects
    }
}

fn start_highlight_thread(
    path: &str,
    theme: Theme,
    queue: Arc<Mutex<VecDeque<IndexedLine>>>,
    cache_updated: Arc<Mutex<bool>>,
    cache: Arc<RwLock<HashMap<usize, Vec<TextEffect>>>>,
) -> Option<()> {
    let extension = Path::new(path).extension()?.to_str()?.to_string();

    thread::spawn(move || {
        let mut internal_cache = HashMap::new();
        let syntax_set: SyntaxSet =
            from_uncompressed_data(include_bytes!("../resources/syntax_definitions.packdump"))
                .unwrap();
        let highlighter = Highlighter::new(&theme);
        let syntax_reference = syntax_set.find_syntax_by_extension(&extension);
        if syntax_reference.is_none() {
            return;
        }

        loop {
            thread::sleep(Duration::from_micros(8333));
            let (start, text) = if let Some(indexed_line) = queue.lock().unwrap().pop_front() {
                (indexed_line.index, indexed_line.text)
            } else {
                continue;
            };

            let index = start / SYNTECT_CACHE_FREQUENCY;

            let (mut parse_state, mut highlight_state) = if index > 0 {
                internal_cache.get(&(index - 1)).cloned().unwrap_or((
                    ParseState::new(syntax_reference.unwrap()),
                    HighlightState::new(&highlighter, ScopeStack::new()),
                ))
            } else {
                (
                    ParseState::new(syntax_reference.unwrap()),
                    HighlightState::new(&highlighter, ScopeStack::new()),
                )
            };

            let mut effects = vec![];
            let mut offset = 0;
            for line in text.split_inclusive(|c| *c == b'\n') {
                let line = unsafe { std::str::from_utf8_unchecked(line) };
                let ops = parse_state.parse_line(line, &syntax_set).unwrap();
                for highlight in
                    RangedHighlightIterator::new(&mut highlight_state, &ops, line, &highlighter)
                {
                    effects.push(TextEffect {
                        kind: TextEffectKind::ForegroundColor(crate::renderer::Color::from_rgb(
                            highlight.0.foreground.r,
                            highlight.0.foreground.g,
                            highlight.0.foreground.b,
                        )),
                        start: offset + highlight.2.start,
                        length: highlight.2.len(),
                    });
                }
                offset += line.len();
            }

            {
                let mut cache = cache.write().unwrap();
                cache.insert(index, effects);
                *cache_updated.lock().unwrap() = true;
            }

            internal_cache.insert(index, (parse_state, highlight_state));
        }
    });

    Some(())
}

fn convert_theme(theme: &crate::theme::Theme) -> Theme {
    Theme {
        name: None,
        author: None,
        settings: syntect::highlighting::ThemeSettings {
            foreground: Some(Color::from(theme.foreground_color)),
            background: Some(Color::from(theme.background_color)),
            caret: Some(Color::from(theme.background_color)),
            selection: Some(Color::from(theme.selection_background_color)),
            selection_foreground: Some(Color::from(theme.foreground_color)),
            ..Default::default()
        },
        scopes: vec![
            ThemeItem {
                scope: ScopeSelectors::from_str("comment, punctuation.definition.comment").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.blue)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("string").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.green)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("constant.numeric").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.orange)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("storage.type.numeric").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.pink)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("constant.language").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.red)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("constant.character, constant.other").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.pink)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("variable.member").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.red)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str(
                    "keyword - keyword.operator, keyword.operator.word",
                )
                .unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.pink)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("storage").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.red)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("storage.type").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.pink)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("entity.name.function").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.blue)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str(
                    "entity.name - (entity.name.section | entity.name.tag | entity.name.label)",
                )
                .unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.orange)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("entity.other.inherited-class").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.blue)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("variable.parameter").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.orange)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("variable.language").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.red)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("entity.name.tag").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.red)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("entity.other.attribute-name").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.pink)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("variable.function, variable.annotation").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.blue)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("support.function, support.macro").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.blue)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("support.constant").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.pink)),
                    background: None,
                    font_style: None,
                },
            },
            ThemeItem {
                scope: ScopeSelectors::from_str("support.type, support.class").unwrap(),
                style: StyleModifier {
                    foreground: Some(Color::from(theme.palette.blue)),
                    background: None,
                    font_style: None,
                },
            },
        ],
    }
}
