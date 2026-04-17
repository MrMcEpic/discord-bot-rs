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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::music::track::Track;

	fn track(title: &str) -> Track {
		Track {
			url: format!("https://example.invalid/{title}"),
			title: title.to_string(),
			duration: 180,
			thumbnail: String::new(),
			requested_by: "tester".to_string(),
		}
	}

	fn player() -> GuildPlayer {
		GuildPlayer::new(GuildId::new(1))
	}

	#[test]
	fn loop_mode_cycle_walks_off_track_queue_off() {
		assert_eq!(LoopMode::Off.cycle(), LoopMode::Track);
		assert_eq!(LoopMode::Track.cycle(), LoopMode::Queue);
		assert_eq!(LoopMode::Queue.cycle(), LoopMode::Off);
	}

	#[test]
	fn new_player_is_empty() {
		let p = player();
		assert!(p.queue.is_empty());
		assert!(p.current.is_none());
		assert_eq!(p.loop_mode, LoopMode::Off);
		assert!(!p.paused);
		assert!(!p.is_full());
	}

	#[test]
	fn enqueue_appends_and_returns_new_length() {
		let mut p = player();
		assert_eq!(p.enqueue(track("a")), 1);
		assert_eq!(p.enqueue(track("b")), 2);
		assert_eq!(p.queue.front().unwrap().title, "a");
	}

	#[test]
	fn is_full_at_max_queue_length() {
		let mut p = player();
		for i in 0..MAX_QUEUE_LENGTH {
			p.enqueue(track(&format!("t{i}")));
		}
		assert!(p.is_full());
		assert_eq!(p.queue.len(), MAX_QUEUE_LENGTH);
	}

	#[test]
	fn enqueue_many_caps_at_remaining_capacity() {
		let mut p = player();
		// Pre-fill so only 3 slots remain.
		for i in 0..(MAX_QUEUE_LENGTH - 3) {
			p.enqueue(track(&format!("seed{i}")));
		}
		let added = p.enqueue_many(vec![track("x"), track("y"), track("z"), track("dropped")]);
		assert_eq!(added, 3);
		assert_eq!(p.queue.len(), MAX_QUEUE_LENGTH);
		// The "dropped" track must not have been silently appended past the cap.
		assert!(p.queue.iter().all(|t| t.title != "dropped"));
	}

	#[test]
	fn enqueue_many_returns_zero_when_already_full() {
		let mut p = player();
		for i in 0..MAX_QUEUE_LENGTH {
			p.enqueue(track(&format!("t{i}")));
		}
		assert_eq!(p.enqueue_many(vec![track("nope")]), 0);
		assert_eq!(p.queue.len(), MAX_QUEUE_LENGTH);
	}

	#[test]
	fn advance_pops_from_queue_when_loop_off() {
		let mut p = player();
		p.enqueue(track("a"));
		p.enqueue(track("b"));
		assert_eq!(p.advance().unwrap().title, "a");
		assert_eq!(p.current.as_ref().unwrap().title, "a");
		assert_eq!(p.advance().unwrap().title, "b");
		assert!(p.advance().is_none());
		// Queue exhausted: current cleared.
		assert!(p.current.is_none());
	}

	#[test]
	fn advance_with_loop_track_repeats_current_indefinitely() {
		let mut p = player();
		p.enqueue(track("a"));
		p.enqueue(track("b"));
		p.advance(); // current = a
		p.loop_mode = LoopMode::Track;
		// Should keep returning "a" without consuming the queue.
		for _ in 0..5 {
			assert_eq!(p.advance().unwrap().title, "a");
		}
		assert_eq!(
			p.queue.len(),
			1,
			"queue must NOT drain under LoopMode::Track"
		);
	}

	#[test]
	fn advance_with_loop_queue_rotates_finished_track_to_back() {
		let mut p = player();
		p.enqueue(track("a"));
		p.enqueue(track("b"));
		p.advance(); // current = a, queue = [b]
		p.loop_mode = LoopMode::Queue;
		assert_eq!(p.advance().unwrap().title, "b"); // a moves to back
		assert_eq!(p.queue.back().unwrap().title, "a");
		assert_eq!(p.advance().unwrap().title, "a"); // b moves to back
		assert_eq!(p.queue.back().unwrap().title, "b");
	}

	#[test]
	fn skip_current_returns_title_of_currently_playing_track() {
		let mut p = player();
		assert!(p.skip_current().is_none());
		p.enqueue(track("a"));
		p.advance();
		assert_eq!(p.skip_current().as_deref(), Some("a"));
	}

	#[test]
	fn stop_all_clears_state_and_resets_loop() {
		let mut p = player();
		p.enqueue(track("a"));
		p.enqueue(track("b"));
		p.advance();
		p.loop_mode = LoopMode::Queue;
		p.paused = true;
		p.stop_all();
		assert!(p.queue.is_empty());
		assert!(p.current.is_none());
		assert_eq!(p.loop_mode, LoopMode::Off);
		assert!(!p.paused);
	}

	#[test]
	fn remove_uses_one_based_position() {
		let mut p = player();
		p.enqueue(track("a"));
		p.enqueue(track("b"));
		p.enqueue(track("c"));
		assert_eq!(p.remove(2).unwrap().title, "b");
		assert_eq!(p.queue.len(), 2);
		assert_eq!(p.queue[0].title, "a");
		assert_eq!(p.queue[1].title, "c");
	}

	#[test]
	fn remove_rejects_out_of_range_positions() {
		let mut p = player();
		p.enqueue(track("a"));
		assert!(p.remove(0).is_none());
		assert!(p.remove(2).is_none());
		assert_eq!(p.queue.len(), 1);
	}

	#[test]
	fn shuffle_no_op_when_fewer_than_two_tracks() {
		let mut p = player();
		assert_eq!(p.shuffle(), 0);
		p.enqueue(track("a"));
		assert_eq!(p.shuffle(), 1);
	}

	#[test]
	fn shuffle_preserves_track_set() {
		let mut p = player();
		let titles = ["a", "b", "c", "d", "e", "f", "g"];
		for t in &titles {
			p.enqueue(track(t));
		}
		assert_eq!(p.shuffle(), titles.len());
		let mut got: Vec<String> = p.queue.iter().map(|t| t.title.clone()).collect();
		got.sort();
		let mut want: Vec<String> = titles.iter().map(|s| s.to_string()).collect();
		want.sort();
		assert_eq!(got, want);
	}

	#[test]
	fn leave_empty_resets_state_like_stop_all() {
		let mut p = player();
		p.enqueue(track("a"));
		p.advance();
		p.loop_mode = LoopMode::Track;
		p.paused = true;
		p.leave_empty();
		assert!(p.queue.is_empty());
		assert!(p.current.is_none());
		assert_eq!(p.loop_mode, LoopMode::Off);
		assert!(!p.paused);
	}

	#[test]
	fn loop_mode_label_strings_are_non_empty() {
		// Sanity check — these surface as user-facing toast text.
		assert!(!LoopMode::Off.label().is_empty());
		assert!(!LoopMode::Track.label().is_empty());
		assert!(!LoopMode::Queue.label().is_empty());
	}
}
