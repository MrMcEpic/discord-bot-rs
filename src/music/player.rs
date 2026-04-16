use rand::seq::SliceRandom;
use serenity::all::GuildId;
use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use super::track::Track;

pub const MAX_QUEUE_LENGTH: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoopMode {
	Off,
	Track,
	Queue,
}

impl LoopMode {
	pub fn cycle(self) -> Self {
		match self {
			Self::Off => Self::Track,
			Self::Track => Self::Queue,
			Self::Queue => Self::Off,
		}
	}

	pub fn label(self) -> &'static str {
		match self {
			Self::Off => "Loop disabled.",
			Self::Track => "Looping **current track**.",
			Self::Queue => "Looping **entire queue**.",
		}
	}
}

pub struct GuildPlayer {
	pub guild_id: GuildId,
	pub queue: VecDeque<Track>,
	pub current: Option<Track>,
	pub loop_mode: LoopMode,
	pub paused: bool,
	/// Set to `true` immediately before we tell songbird to stop the currently
	/// playing track so we can start a new one (e.g. on `!m skip`, button skip,
	/// or AI tool-driven track change). The track-end event handler swaps it
	/// back to `false` and returns early — otherwise it would advance the queue
	/// a second time on top of the new track we just started.
	///
	/// `Arc<AtomicBool>` so the handler (which only holds a `PlaybackContext`)
	/// can read it without re-locking the player mutex from inside its own
	/// `act` body, and so cloning the player handle around is cheap.
	pub skip_in_progress: Arc<AtomicBool>,
}

impl GuildPlayer {
	pub fn new(guild_id: GuildId) -> Self {
		Self {
			guild_id,
			queue: VecDeque::new(),
			current: None,
			loop_mode: LoopMode::Off,
			paused: false,
			skip_in_progress: Arc::new(AtomicBool::new(false)),
		}
	}

	pub fn is_full(&self) -> bool {
		self.queue.len() >= MAX_QUEUE_LENGTH
	}

	pub fn enqueue(&mut self, track: Track) -> usize {
		self.queue.push_back(track);
		self.queue.len()
	}

	/// Enqueue many tracks, respecting max queue length. Returns count added.
	pub fn enqueue_many(&mut self, tracks: Vec<Track>) -> usize {
		let available = MAX_QUEUE_LENGTH.saturating_sub(self.queue.len());
		let mut count = 0;
		for track in tracks.into_iter().take(available) {
			self.queue.push_back(track);
			count += 1;
		}
		count
	}

	/// Determine the next track to play based on loop mode.
	/// Returns the track to play, or None if queue is exhausted.
	pub fn advance(&mut self) -> Option<Track> {
		if self.loop_mode == LoopMode::Track {
			if let Some(track) = &self.current {
				return Some(track.clone());
			}
		}

		if self.loop_mode == LoopMode::Queue {
			if let Some(current) = self.current.take() {
				self.queue.push_back(current);
			}
		} else {
			self.current = None;
		}

		if let Some(track) = self.queue.pop_front() {
			self.current = Some(track.clone());
			Some(track)
		} else {
			self.current = None;
			None
		}
	}

	pub fn skip_current(&mut self) -> Option<String> {
		self.current.as_ref().map(|t| t.title.clone())
	}

	pub fn stop_all(&mut self) {
		self.queue.clear();
		self.current = None;
		self.loop_mode = LoopMode::Off;
		self.paused = false;
	}

	pub fn remove(&mut self, position: usize) -> Option<Track> {
		if position >= 1 && position <= self.queue.len() {
			self.queue.remove(position - 1)
		} else {
			None
		}
	}

	pub fn shuffle(&mut self) -> usize {
		let len = self.queue.len();
		if len < 2 {
			return len;
		}
		let mut vec: Vec<Track> = self.queue.drain(..).collect();
		vec.shuffle(&mut rand::rng());
		self.queue = VecDeque::from(vec);
		len
	}

	pub fn leave_empty(&mut self) {
		self.queue.clear();
		self.current = None;
		self.loop_mode = LoopMode::Off;
		self.paused = false;
	}
}
