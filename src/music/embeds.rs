use serenity::builder::{CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter};
use serenity::model::prelude::*;

use super::player::LoopMode;
use super::track::Track;
use crate::util::duration::format_track_duration;

pub fn music_controls(paused: bool, loop_mode: LoopMode) -> Vec<CreateActionRow> {
	let pause_btn = CreateButton::new("music_pauseresume")
		.emoji(if paused {
			ReactionType::Unicode("▶️".to_string())
		} else {
			ReactionType::Unicode("⏸️".to_string())
		})
		.style(if paused {
			ButtonStyle::Success
		} else {
			ButtonStyle::Secondary
		});

	let skip_btn = CreateButton::new("music_skip")
		.emoji(ReactionType::Unicode("⏭️".to_string()))
		.style(ButtonStyle::Secondary);

	let stop_btn = CreateButton::new("music_stop")
		.emoji(ReactionType::Unicode("⏹️".to_string()))
		.style(ButtonStyle::Danger);

	let shuffle_btn = CreateButton::new("music_shuffle")
		.emoji(ReactionType::Unicode("🔀".to_string()))
		.style(ButtonStyle::Secondary);

	let loop_emoji = match loop_mode {
		LoopMode::Off | LoopMode::Queue => "🔁",
		LoopMode::Track => "🔂",
	};
	let loop_style = match loop_mode {
		LoopMode::Off => ButtonStyle::Secondary,
		LoopMode::Track => ButtonStyle::Success,
		LoopMode::Queue => ButtonStyle::Primary,
	};
	let loop_btn = CreateButton::new("music_loop")
		.emoji(ReactionType::Unicode(loop_emoji.to_string()))
		.style(loop_style);

	let row1 = CreateActionRow::Buttons(vec![pause_btn, skip_btn, stop_btn, shuffle_btn, loop_btn]);

	let queue_btn = CreateButton::new("music_queue")
		.label("Queue")
		.emoji(ReactionType::Unicode("📋".to_string()))
		.style(ButtonStyle::Secondary);

	let row2 = CreateActionRow::Buttons(vec![queue_btn]);

	vec![row1, row2]
}

pub fn now_playing_embed(track: &Track) -> CreateEmbed {
	let mut embed = CreateEmbed::new()
		.color(0x5865f2)
		.title("Now Playing")
		.description(format!("**[{}]({})**", track.title, track.url))
		.field("Duration", format_track_duration(track.duration), true)
		.field("Requested by", &track.requested_by, true);

	if !track.thumbnail.is_empty() {
		embed = embed.thumbnail(&track.thumbnail);
	}

	embed
}

pub fn added_to_queue_embed(track: &Track, position: usize) -> CreateEmbed {
	let mut embed = CreateEmbed::new()
		.color(0x57f287)
		.title("Added to Queue")
		.description(format!("**[{}]({})**", track.title, track.url))
		.field("Duration", format_track_duration(track.duration), true)
		.field("Position", format!("#{position}"), true)
		.field("Requested by", &track.requested_by, true);

	if !track.thumbnail.is_empty() {
		embed = embed.thumbnail(&track.thumbnail);
	}

	embed
}

pub fn queue_embed(current: Option<&Track>, queue: &[Track]) -> CreateEmbed {
	let mut embed = CreateEmbed::new().color(0x5865f2).title("Music Queue");

	if let Some(track) = current {
		embed = embed.description(format!(
			"**Now Playing:** [{}]({}) `{}`",
			track.title,
			track.url,
			format_track_duration(track.duration)
		));
	} else {
		embed = embed.description("Nothing is playing right now.");
	}

	if !queue.is_empty() {
		let mut lines: Vec<String> = queue
			.iter()
			.take(15)
			.enumerate()
			.map(|(i, track)| {
				format!(
					"**{}.** [{}]({}) `{}` — {}",
					i + 1,
					track.title,
					track.url,
					format_track_duration(track.duration),
					track.requested_by
				)
			})
			.collect();

		if queue.len() > 15 {
			lines.push(format!("... and {} more", queue.len() - 15));
		}

		let total_duration: u64 =
			queue.iter().map(|t| t.duration).sum::<u64>() + current.map_or(0, |t| t.duration);

		embed = embed.field(
			format!(
				"Up Next ({} song{}) — Total: {}",
				queue.len(),
				if queue.len() == 1 { "" } else { "s" },
				format_track_duration(total_duration)
			),
			lines.join("\n"),
			false,
		);
	} else if current.is_some() {
		embed = embed.field("Up Next", "Queue is empty.", false);
	}

	embed
}

pub fn status_footer(paused: bool, loop_mode: LoopMode) -> Option<CreateEmbedFooter> {
	let mut parts = Vec::new();
	if paused {
		parts.push("Paused");
	}
	match loop_mode {
		LoopMode::Track => parts.push("Looping Track"),
		LoopMode::Queue => parts.push("Looping Queue"),
		LoopMode::Off => {}
	}
	if parts.is_empty() {
		None
	} else {
		Some(CreateEmbedFooter::new(parts.join(" • ")))
	}
}
