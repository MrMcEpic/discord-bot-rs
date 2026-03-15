use serenity::builder::CreateEmbed;

use super::game::{LetterResult, WordleGame};

pub fn game_embed(game: &WordleGame) -> CreateEmbed {
    let mut grid = String::new();

    // Show guessed rows
    for (word, results) in &game.guesses {
        let squares: String = results.iter().map(|r| r.emoji()).collect();
        let upper = word.to_uppercase();
        grid.push_str(&format!("{squares}  **{upper}**\n"));
    }

    // Show empty rows
    let remaining = game.max_guesses as usize - game.guesses.len();
    for _ in 0..remaining {
        grid.push_str("⬜⬜⬜⬜⬜\n");
    }

    // Keyboard tracker
    let kb = game.keyboard_state();
    let mut correct_letters: Vec<char> = Vec::new();
    let mut present_letters: Vec<char> = Vec::new();
    let mut absent_letters: Vec<char> = Vec::new();

    for ch in 'a'..='z' {
        match kb.get(&ch) {
            Some(LetterResult::Correct) => correct_letters.push(ch),
            Some(LetterResult::Present) => present_letters.push(ch),
            Some(LetterResult::Absent) => absent_letters.push(ch),
            None => {}
        }
    }

    let mut keyboard = String::new();
    if !correct_letters.is_empty() {
        let letters: String = correct_letters.iter().map(|c| c.to_uppercase().to_string()).collect::<Vec<_>>().join(" ");
        keyboard.push_str(&format!("🟩 {letters}  "));
    }
    if !present_letters.is_empty() {
        let letters: String = present_letters.iter().map(|c| c.to_uppercase().to_string()).collect::<Vec<_>>().join(" ");
        keyboard.push_str(&format!("🟨 {letters}  "));
    }
    if !absent_letters.is_empty() {
        let letters: String = absent_letters.iter().map(|c| c.to_uppercase().to_string()).collect::<Vec<_>>().join(" ");
        keyboard.push_str(&format!("⬛ {letters}"));
    }

    let mut description = grid;
    if !keyboard.is_empty() {
        description.push_str(&format!("\n{keyboard}"));
    }

    CreateEmbed::new()
        .color(0x538d4e) // Wordle green
        .title(format!("Wordle — {}", game.puzzle_date))
        .description(description)
}

pub fn game_over_embed(game: &WordleGame, won: bool) -> CreateEmbed {
    let mut grid = String::new();
    for (word, results) in &game.guesses {
        let squares: String = results.iter().map(|r| r.emoji()).collect();
        let upper = word.to_uppercase();
        grid.push_str(&format!("{squares}  **{upper}**\n"));
    }

    let (title, color) = if won {
        let tries = game.guesses.len();
        (
            format!("Wordle — {} — Solved in {}! 🎉", game.puzzle_date, tries),
            0x538d4e, // green
        )
    } else {
        grid.push_str(&format!(
            "\nThe answer was **{}**",
            game.solution.to_uppercase()
        ));
        (
            format!("Wordle — {} — Game Over", game.puzzle_date),
            0xed4245, // red
        )
    };

    CreateEmbed::new()
        .color(color)
        .title(title)
        .description(grid)
}
