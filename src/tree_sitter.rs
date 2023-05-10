use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

use crate::{language_support::Language, renderer::TextEffect, theme::Theme};

const HIGHLIGHT_NAMES: [&str; 9] = [
    "keyword",
    "type.builtin",
    "type",
    "string",
    "comment",
    "function",
    "function.method",
    "constant.builtin",
    "constant",
];

pub struct TreeSitter {
    configuration: HighlightConfiguration,
    highlighter: Highlighter,
}

impl TreeSitter {
    pub fn new(source_language: &Language) -> Option<Self> {
        let (language, query) = match source_language.identifier {
            "cpp" => (tree_sitter_c::language(), tree_sitter_c::HIGHLIGHT_QUERY),
            "rust" => (
                tree_sitter_rust::language(),
                tree_sitter_rust::HIGHLIGHT_QUERY,
            ),
            _ => return None,
        };

        let mut configuration = HighlightConfiguration::new(language, query, "", "").ok()?;
        configuration.configure(&HIGHLIGHT_NAMES);

        let highlighter = Highlighter::new();

        Some(Self {
            configuration,
            highlighter,
        })
    }

    pub fn highlight_text(
        &mut self,
        text: &[u8],
        offset: usize,
        length: usize,
        theme: &Theme,
    ) -> Vec<TextEffect> {
        let mut effects = vec![];

        if let Ok(highlights) = self
            .highlighter
            .highlight(&self.configuration, text, None, |_| None)
        {
            let mut color = None;

            for highlight in highlights.flatten() {
                match highlight {
                    HighlightEvent::HighlightStart(style) => {
                        color = Some(theme.tree_sitter_colors[style.0]);
                    }
                    HighlightEvent::HighlightEnd => color = None,
                    HighlightEvent::Source { start, end } => {
                        if end > offset && end < offset + length {
                            let start = if start > offset { start - offset } else { 0 };
                            let end = end - offset;
                            if let Some(color) = color {
                                effects.push(TextEffect {
                                    kind: crate::renderer::TextEffectKind::ForegroundColor(color),
                                    start,
                                    length: end - start,
                                });
                            }
                        }
                    }
                }
            }
        }

        effects
    }
}
