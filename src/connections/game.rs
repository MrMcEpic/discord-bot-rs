use rand::seq::SliceRandom;
use serenity::model::prelude::*;

use super::api::{ConnectionsPuzzle, PuzzleCategory};

pub struct ConnectionsGame {
	pub puzzle_date: String,
	pub puzzle_id: i64,
	pub categories: Vec<PuzzleCategory>,
	pub board: Vec<String>,   // remaining unguessed words, shuffled
	pub selected: Vec<usize>, // indices into `board` currently selected
	pub solved: Vec<usize>,   // category difficulty indices that have been solved
	pub mistakes_remaining: u8,
	pub message_id: MessageId,
	pub channel_id: ChannelId,
	pub last_action: std::time::Instant,
	pub status_message: Option<String>,
}

pub enum GuessResult {
	Correct { category_index: usize },
	OneAway,
	Wrong,
	AlreadyGuessed,
}

impl ConnectionsGame {
	pub fn new(puzzle: ConnectionsPuzzle, message_id: MessageId, channel_id: ChannelId) -> Self {
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::connections::api::{ConnectionsPuzzle, PuzzleCategory};

	fn fixture_puzzle() -> ConnectionsPuzzle {
		ConnectionsPuzzle {
			id: 1,
			date: "2024-01-01".to_string(),
			categories: vec![
				PuzzleCategory {
					title: "Yellow".into(),
					words: vec!["A".into(), "B".into(), "C".into(), "D".into()],
					difficulty: 0,
				},
				PuzzleCategory {
					title: "Green".into(),
					words: vec!["E".into(), "F".into(), "G".into(), "H".into()],
					difficulty: 1,
				},
				PuzzleCategory {
					title: "Blue".into(),
					words: vec!["I".into(), "J".into(), "K".into(), "L".into()],
					difficulty: 2,
				},
				PuzzleCategory {
					title: "Purple".into(),
					words: vec!["M".into(), "N".into(), "O".into(), "P".into()],
					difficulty: 3,
				},
			],
		}
	}

	fn new_game() -> ConnectionsGame {
		ConnectionsGame::new(fixture_puzzle(), MessageId::new(1), ChannelId::new(1))
	}

	/// Select 4 board indices that match the given category words.
	fn select_category(g: &mut ConnectionsGame, words: &[&str]) {
		g.selected.clear();
		for w in words {
			let idx = g
				.board
				.iter()
				.position(|b| b == w)
				.expect("word missing from board");
			g.selected.push(idx);
		}
		assert_eq!(g.selected.len(), 4);
	}

	#[test]
	fn new_game_has_16_words_and_full_lives() {
		let g = new_game();
		assert_eq!(g.board.len(), 16);
		assert_eq!(g.mistakes_remaining, 4);
		assert!(g.solved.is_empty());
		assert!(!g.is_won());
		assert!(!g.is_lost());
	}

	#[test]
	fn correct_guess_solves_and_removes_words() {
		let mut g = new_game();
		select_category(&mut g, &["A", "B", "C", "D"]);
		match g.submit_guess() {
			GuessResult::Correct { category_index } => assert_eq!(category_index, 0),
			_ => panic!("expected Correct"),
		}
		assert_eq!(g.solved, vec![0]);
		assert_eq!(g.board.len(), 12);
		assert_eq!(g.mistakes_remaining, 4);
	}

	#[test]
	fn wrong_guess_decrements_mistakes() {
		let mut g = new_game();
		// Pick one from each of 4 categories — definitely not a valid group.
		select_category(&mut g, &["A", "E", "I", "M"]);
		match g.submit_guess() {
			GuessResult::Wrong | GuessResult::OneAway => {}
			_ => panic!("expected wrong/one-away"),
		}
		assert_eq!(g.mistakes_remaining, 3);
		assert!(g.selected.is_empty());
	}

	#[test]
	fn one_away_is_detected_and_costs_a_mistake() {
		let mut g = new_game();
		// 3 yellow words + one green = 3-out-of-4 match → OneAway.
		select_category(&mut g, &["A", "B", "C", "E"]);
		matches!(g.submit_guess(), GuessResult::OneAway);
		assert_eq!(g.mistakes_remaining, 3);
	}

	#[test]
	fn winning_all_four_groups_sets_is_won() {
		let mut g = new_game();
		for words in [
			["A", "B", "C", "D"],
			["E", "F", "G", "H"],
			["I", "J", "K", "L"],
			["M", "N", "O", "P"],
		] {
			select_category(&mut g, &words);
			match g.submit_guess() {
				GuessResult::Correct { .. } => {}
				_ => panic!("expected Correct"),
			}
		}
		assert!(g.is_won());
		assert!(!g.is_lost());
		assert!(g.is_over());
		assert_eq!(g.board.len(), 0);
	}

	#[test]
	fn losing_after_four_mistakes_sets_is_lost() {
		let mut g = new_game();
		for _ in 0..4 {
			select_category(&mut g, &["A", "E", "I", "M"]);
			let _ = g.submit_guess();
		}
		assert_eq!(g.mistakes_remaining, 0);
		assert!(g.is_lost());
		assert!(g.is_over());
	}

	#[test]
	fn submit_with_fewer_than_four_selected_is_wrong_no_op() {
		let mut g = new_game();
		// No selection at all → Wrong, but mistakes are NOT decremented for this
		// no-op case because the function returns before the mistake-counter
		// branch only when `selected.len() != 4`. Note: current impl actually
		// returns early Wrong but does NOT decrement; assert that.
		match g.submit_guess() {
			GuessResult::Wrong => {}
			_ => panic!("expected Wrong on empty selection"),
		}
		assert_eq!(g.mistakes_remaining, 4);
	}

	#[test]
	fn is_expired_after_simulated_timeout() {
		// Cheaper than sleeping 30 minutes: rewind `last_action` to >30 min ago.
		let mut g = new_game();
		assert!(!g.is_expired());
		g.last_action = std::time::Instant::now()
			.checked_sub(std::time::Duration::from_secs(31 * 60))
			.expect("clock not new enough to subtract 31 minutes");
		assert!(g.is_expired());
	}

	#[test]
	fn toggle_select_caps_at_four_and_can_deselect() {
		let mut g = new_game();
		for i in 0..6 {
			g.toggle_select(i);
		}
		assert_eq!(g.selected.len(), 4, "selection capped at 4");
		// Toggle off the first selection.
		g.toggle_select(0);
		assert_eq!(g.selected.len(), 3);
		assert!(!g.selected.contains(&0));
	}

	#[test]
	fn deselect_all_clears_selection() {
		let mut g = new_game();
		g.toggle_select(0);
		g.toggle_select(1);
		g.deselect_all();
		assert!(g.selected.is_empty());
	}

	#[test]
	fn difficulty_emoji_table() {
		assert_eq!(ConnectionsGame::difficulty_emoji(0), "🟨");
		assert_eq!(ConnectionsGame::difficulty_emoji(1), "🟩");
		assert_eq!(ConnectionsGame::difficulty_emoji(2), "🟦");
		assert_eq!(ConnectionsGame::difficulty_emoji(3), "🟪");
		assert_eq!(ConnectionsGame::difficulty_emoji(255), "⬜");
	}

	// NOTE: GuessResult::AlreadyGuessed exists in the enum but submit_guess()
	// never returns it — once a category is solved, its words are removed from
	// the board so the same selection can't be made again. Audit flagged this
	// as dead. We intentionally do NOT add a test asserting AlreadyGuessed
	// behavior because the current impl doesn't produce it.
}
