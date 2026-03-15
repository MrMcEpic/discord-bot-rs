use serenity::model::prelude::*;
use std::collections::HashMap;
use std::sync::LazyLock;

static VALID_WORDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    include_str!("words.txt")
        .lines()
        .filter(|l| !l.is_empty())
        .collect()
});

pub fn is_valid_word(word: &str) -> bool {
    VALID_WORDS.binary_search(&word).is_ok() || VALID_WORDS.contains(&word)
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
    pub fn new(solution: String, date: String, message_id: MessageId, channel_id: ChannelId) -> Self {
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
                    (LetterResult::Present, LetterResult::Correct) => *entry = LetterResult::Correct,
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
