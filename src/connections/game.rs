use rand::seq::SliceRandom;
use serenity::model::prelude::*;

use super::api::{ConnectionsPuzzle, PuzzleCategory};

pub struct ConnectionsGame {
    pub puzzle_date: String,
    pub puzzle_id: i64,
    pub categories: Vec<PuzzleCategory>,
    pub board: Vec<String>,       // remaining unguessed words, shuffled
    pub selected: Vec<usize>,     // indices into `board` currently selected
    pub solved: Vec<usize>,       // category difficulty indices that have been solved
    pub mistakes_remaining: u8,
    pub message_id: MessageId,
    pub channel_id: ChannelId,
    pub last_action: std::time::Instant,
    pub status_message: Option<String>,
}

pub enum GuessResult {
    Correct {
        category_index: usize,
    },
    OneAway,
    Wrong,
    AlreadyGuessed,
}

impl ConnectionsGame {
    pub fn new(
        puzzle: ConnectionsPuzzle,
        message_id: MessageId,
        channel_id: ChannelId,
    ) -> Self {
        let mut board: Vec<String> = puzzle
            .categories
            .iter()
            .flat_map(|cat| cat.words.iter().cloned())
            .collect();
        board.shuffle(&mut rand::thread_rng());

        Self {
            puzzle_date: puzzle.date,
            puzzle_id: puzzle.id,
            categories: puzzle.categories,
            board,
            selected: Vec::new(),
            solved: Vec::new(),
            mistakes_remaining: 4,
            message_id,
            channel_id,
            last_action: std::time::Instant::now(),
            status_message: None,
        }
    }

    pub fn toggle_select(&mut self, index: usize) {
        self.last_action = std::time::Instant::now();
        if let Some(pos) = self.selected.iter().position(|&i| i == index) {
            self.selected.remove(pos);
        } else if self.selected.len() < 4 && index < self.board.len() {
            self.selected.push(index);
        }
    }

    pub fn deselect_all(&mut self) {
        self.last_action = std::time::Instant::now();
        self.selected.clear();
        self.status_message = None;
    }

    pub fn shuffle_board(&mut self) {
        self.last_action = std::time::Instant::now();
        self.board.shuffle(&mut rand::thread_rng());
        self.selected.clear();
        self.status_message = None;
    }

    pub fn submit_guess(&mut self) -> GuessResult {
        self.last_action = std::time::Instant::now();

        if self.selected.len() != 4 {
            return GuessResult::Wrong;
        }

        let selected_words: Vec<String> = self
            .selected
            .iter()
            .map(|&i| self.board[i].clone())
            .collect();

        // Check each unsolved category
        for (cat_idx, cat) in self.categories.iter().enumerate() {
            if self.solved.contains(&cat_idx) {
                continue;
            }

            let mut cat_words = cat.words.clone();
            cat_words.sort();
            let mut guess_words = selected_words.clone();
            guess_words.sort();

            if cat_words == guess_words {
                // Correct! Remove these words from board
                self.solved.push(cat_idx);

                // Remove words from board (in reverse index order to preserve indices)
                let mut indices = self.selected.clone();
                indices.sort_unstable_by(|a, b| b.cmp(a));
                for idx in indices {
                    self.board.remove(idx);
                }
                self.selected.clear();

                return GuessResult::Correct {
                    category_index: cat_idx,
                };
            }
        }

        // Check for "one away" (3 out of 4 match any unsolved category)
        let one_away = self.categories.iter().enumerate().any(|(cat_idx, cat)| {
            if self.solved.contains(&cat_idx) {
                return false;
            }
            let matching = selected_words
                .iter()
                .filter(|w| cat.words.contains(w))
                .count();
            matching == 3
        });

        self.mistakes_remaining = self.mistakes_remaining.saturating_sub(1);
        self.selected.clear();

        if one_away {
            GuessResult::OneAway
        } else {
            GuessResult::Wrong
        }
    }

    pub fn is_won(&self) -> bool {
        self.solved.len() == 4
    }

    pub fn is_lost(&self) -> bool {
        self.mistakes_remaining == 0
    }

    pub fn is_over(&self) -> bool {
        self.is_won() || self.is_lost()
    }

    pub fn is_expired(&self) -> bool {
        self.last_action.elapsed() > std::time::Duration::from_secs(30 * 60)
    }

    pub fn difficulty_emoji(difficulty: u8) -> &'static str {
        match difficulty {
            0 => "🟨",
            1 => "🟩",
            2 => "🟦",
            3 => "🟪",
            _ => "⬜",
        }
    }
}
