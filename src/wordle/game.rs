use serenity::model::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

static VALID_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
	include_str!("words.txt")
		.lines()
		.filter(|l| !l.is_empty())
		.collect()
});

pub fn is_valid_word(word: &str) -> bool {
	VALID_WORDS.contains(word)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LetterResult {
	Correct, // 🟩
	Present, // 🟨
	Absent,  // ⬛
}

impl LetterResult {
	pub fn emoji(&self) -> &'static str {
		match self {
			Self::Correct => "🟩",
			Self::Present => "🟨",
			Self::Absent => "⬛",
		}
	}
}

pub struct WordleGame {
	pub solution: String,
	pub puzzle_date: String,
	pub guesses: Vec<(String, Vec<LetterResult>)>,
	pub max_guesses: u8,
	pub message_id: MessageId,
	pub channel_id: ChannelId,
	pub last_action: std::time::Instant,
}

pub enum GuessOutcome {
	Continue,
	Won,
	Lost,
}

impl WordleGame {
	pub fn new(
		solution: String,
		date: String,
		message_id: MessageId,
		channel_id: ChannelId,
	) -> Self {
		Self {
			solution,
			puzzle_date: date,
			guesses: Vec::new(),
			max_guesses: 6,
			message_id,
			channel_id,
			last_action: std::time::Instant::now(),
		}
	}

	pub fn make_guess(&mut self, guess: &str) -> GuessOutcome {
		self.last_action = std::time::Instant::now();
		let guess = guess.to_lowercase();
		let results = evaluate_guess(&guess, &self.solution);
		let won = results.iter().all(|r| *r == LetterResult::Correct);
		self.guesses.push((guess, results));

		if won {
			GuessOutcome::Won
		} else if self.guesses.len() >= self.max_guesses as usize {
			GuessOutcome::Lost
		} else {
			GuessOutcome::Continue
		}
	}

	pub fn is_over(&self) -> bool {
		if self.guesses.is_empty() {
			return false;
		}
		let last = &self.guesses.last().unwrap().1;
		let won = last.iter().all(|r| *r == LetterResult::Correct);
		won || self.guesses.len() >= self.max_guesses as usize
	}

	pub fn is_expired(&self) -> bool {
		self.last_action.elapsed() > std::time::Duration::from_secs(30 * 60)
	}

	/// Returns keyboard state: maps each guessed letter to its best known result.
	pub fn keyboard_state(&self) -> HashMap<char, LetterResult> {
		let mut state: HashMap<char, LetterResult> = HashMap::new();
		for (word, results) in &self.guesses {
			for (ch, result) in word.chars().zip(results.iter()) {
				let entry = state.entry(ch).or_insert(LetterResult::Absent);
				// Upgrade: Absent < Present < Correct
				match (*entry, result) {
					(LetterResult::Absent, _) => *entry = *result,
					(LetterResult::Present, LetterResult::Correct) => {
						*entry = LetterResult::Correct
					}
					_ => {}
				}
			}
		}
		state
	}
}

fn evaluate_guess(guess: &str, solution: &str) -> Vec<LetterResult> {
	let guess_chars: Vec<char> = guess.chars().collect();
	let solution_chars: Vec<char> = solution.chars().collect();
	let mut results = vec![LetterResult::Absent; 5];

	// Track which solution positions are "used"
	let mut used = [false; 5];

	// First pass: find exact matches (🟩)
	for i in 0..5 {
		if guess_chars[i] == solution_chars[i] {
			results[i] = LetterResult::Correct;
			used[i] = true;
		}
	}

	// Second pass: find present-but-misplaced (🟨)
	for i in 0..5 {
		if results[i] == LetterResult::Correct {
			continue;
		}
		for j in 0..5 {
			if !used[j] && guess_chars[i] == solution_chars[j] {
				results[i] = LetterResult::Present;
				used[j] = true;
				break;
			}
		}
	}

	results
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn is_valid_word_accepts_known_word() {
		// "speed" is present in words.txt (verified from file).
		assert!(is_valid_word("speed"));
		assert!(is_valid_word("array"));
	}

	#[test]
	fn is_valid_word_rejects_unknown_word() {
		assert!(!is_valid_word("zzzzz"));
		assert!(!is_valid_word("xxxxx"));
	}

	#[test]
	fn is_valid_word_is_case_sensitive() {
		// The word list is lowercase; uppercase lookup should fail.
		assert!(is_valid_word("speed"));
		assert!(!is_valid_word("SPEED"));
		assert!(!is_valid_word("Speed"));
	}

	#[test]
	fn is_valid_word_rejects_wrong_length() {
		assert!(!is_valid_word(""));
		assert!(!is_valid_word("cat"));
		assert!(!is_valid_word("elephant"));
	}

	#[test]
	fn letter_result_emoji_mapping() {
		assert_eq!(LetterResult::Correct.emoji(), "🟩");
		assert_eq!(LetterResult::Present.emoji(), "🟨");
		assert_eq!(LetterResult::Absent.emoji(), "⬛");
	}

	#[test]
	fn evaluate_guess_all_correct() {
		let r = evaluate_guess("speed", "speed");
		assert!(r.iter().all(|x| *x == LetterResult::Correct));
	}

	#[test]
	fn evaluate_guess_all_absent() {
		// "qwrty" vs "aaaaa" — no shared letters.
		let r = evaluate_guess("qwrty", "aaaaa");
		assert!(r.iter().all(|x| *x == LetterResult::Absent));
	}

	#[test]
	fn evaluate_guess_present_misplaced() {
		// Solution "abcde", guess "eabcd" — every letter present, none in
		// the right spot.
		let r = evaluate_guess("eabcd", "abcde");
		assert!(r.iter().all(|x| *x == LetterResult::Present));
	}

	#[test]
	fn evaluate_guess_handles_duplicate_letters() {
		// Guess "allee" vs solution "apple":
		//   index:    0 1 2 3 4
		//   guess:    a l l e e
		//   solution: a p p l e
		//
		// Pass 1 (exact matches): [0] a==a → Correct (used[0]),
		//   [4] e==e → Correct (used[4]).
		// Pass 2 (present-but-misplaced, single-use):
		//   [1] l → solution[3]='l' unused → Present, used[3]=true.
		//   [2] l → no more unused 'l' in solution → Absent.
		//   [3] e → solution[4]='e' but already used → Absent.
		// This verifies that a duplicate guess letter doesn't double-claim
		// a single solution letter.
		let r = evaluate_guess("allee", "apple");
		assert_eq!(r[0], LetterResult::Correct);
		assert_eq!(r[1], LetterResult::Present);
		assert_eq!(r[2], LetterResult::Absent);
		assert_eq!(r[3], LetterResult::Absent);
		assert_eq!(r[4], LetterResult::Correct);
	}
}
