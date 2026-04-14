use serenity::builder::{CreateActionRow, CreateButton, CreateEmbed};
use serenity::model::prelude::*;

use super::game::ConnectionsGame;

pub fn game_embed(game: &ConnectionsGame) -> CreateEmbed {
	let mut description = String::new();

	// Show solved groups at top
	let mut solved_sorted: Vec<usize> = game.solved.clone();
	solved_sorted.sort();
	for &cat_idx in &solved_sorted {
		let cat = &game.categories[cat_idx];
		let emoji = ConnectionsGame::difficulty_emoji(cat.difficulty);
		let words = cat.words.join(", ");
		description.push_str(&format!("{emoji} **{}**: {}\n", cat.title, words));
	}

	if !description.is_empty() && !game.board.is_empty() {
		description.push('\n');
	}

	// Mistakes indicator
	let mistakes_dots = "⬛".repeat(game.mistakes_remaining as usize)
		+ &"✖️".repeat(4 - game.mistakes_remaining as usize);
	description.push_str(&format!("Mistakes remaining: {mistakes_dots}"));

	// Status message (correct/wrong feedback)
	if let Some(ref msg) = game.status_message {
		description.push_str(&format!("\n\n{msg}"));
	}

	CreateEmbed::new()
		.color(0x5865f2)
		.title(format!("Connections — {}", game.puzzle_date))
		.description(description)
}

pub fn game_over_embed(game: &ConnectionsGame, won: bool) -> CreateEmbed {
	let mut description = String::new();

	// Show all groups in difficulty order
	let mut all_cats: Vec<(usize, &super::api::PuzzleCategory)> =
		game.categories.iter().enumerate().collect();
	all_cats.sort_by_key(|(_, c)| c.difficulty);

	for (_, cat) in &all_cats {
		let emoji = ConnectionsGame::difficulty_emoji(cat.difficulty);
		let words = cat.words.join(", ");
		description.push_str(&format!("{emoji} **{}**: {}\n", cat.title, words));
	}

	let title = if won {
		format!("Connections — {} — Solved! 🎉", game.puzzle_date)
	} else {
		format!("Connections — {} — Game Over", game.puzzle_date)
	};

	let color = if won { 0x57f287 } else { 0xed4245 };

	let mistakes_used = 4 - game.mistakes_remaining;
	description.push_str(&format!(
		"\n{} mistake{}",
		mistakes_used,
		if mistakes_used == 1 { "" } else { "s" }
	));

	CreateEmbed::new()
		.color(color)
		.title(title)
		.description(description)
}

pub fn game_buttons(game: &ConnectionsGame) -> Vec<CreateActionRow> {
	if game.is_over() {
		return vec![];
	}

	let mut rows = Vec::new();

	// Word buttons — 4 per row
	for row_start in (0..game.board.len()).step_by(4) {
		let row_end = (row_start + 4).min(game.board.len());
		let buttons: Vec<CreateButton> = (row_start..row_end)
			.map(|i| {
				let is_selected = game.selected.contains(&i);
				let style = if is_selected {
					ButtonStyle::Primary
				} else {
					ButtonStyle::Secondary
				};
				CreateButton::new(format!("game_word_{i}"))
					.label(&game.board[i])
					.style(style)
			})
			.collect();

		if !buttons.is_empty() {
			rows.push(CreateActionRow::Buttons(buttons));
		}
	}

	// Control row
	let shuffle_btn = CreateButton::new("game_shuffle")
		.label("Shuffle")
		.emoji(ReactionType::Unicode("🔀".to_string()))
		.style(ButtonStyle::Secondary);

	let deselect_btn = CreateButton::new("game_deselect")
		.label("Deselect")
		.style(ButtonStyle::Secondary)
		.disabled(game.selected.is_empty());

	let submit_btn = CreateButton::new("game_submit")
		.label("Submit")
		.emoji(ReactionType::Unicode("✅".to_string()))
		.style(ButtonStyle::Success)
		.disabled(game.selected.len() != 4);

	rows.push(CreateActionRow::Buttons(vec![
		shuffle_btn,
		deselect_btn,
		submit_btn,
	]));

	rows
}
