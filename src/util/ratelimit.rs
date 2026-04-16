use dashmap::DashMap;
use std::time::Instant;

pub struct SlidingWindowLimiter {
	buckets: DashMap<String, Vec<Instant>>,
	max_requests: usize,
	window: std::time::Duration,
}

impl SlidingWindowLimiter {
	pub fn new(max_requests: usize, window: std::time::Duration) -> Self {
		Self {
			buckets: DashMap::new(),
			max_requests,
			window,
		}
	}

	/// Returns seconds until reset if rate limited, or 0 if allowed.
	pub fn check(&self, key: &str) -> u64 {
		let now = Instant::now();
		let mut entry = self.buckets.entry(key.to_string()).or_default();
		let timestamps = entry.value_mut();

		// Prune old timestamps
		timestamps.retain(|t| now.duration_since(*t) < self.window);

		if timestamps.len() >= self.max_requests {
			let oldest = timestamps[0];
			let reset_at = oldest + self.window;
			return reset_at.duration_since(now).as_secs() + 1;
		}

		timestamps.push(now);
		0
	}

	/// Remove empty buckets
	pub fn cleanup(&self) {
		let now = Instant::now();
		self.buckets.retain(|_, timestamps| {
			timestamps.retain(|t| now.duration_since(*t) < self.window);
			!timestamps.is_empty()
		});
	}
}

pub struct RateLimiters {
	pub ai: SlidingWindowLimiter,
	pub music: SlidingWindowLimiter,
	pub moderation: SlidingWindowLimiter,
	pub stocks: SlidingWindowLimiter,
	pub welcome: SlidingWindowLimiter,
}

impl RateLimiters {
	pub fn new() -> Self {
		Self {
			ai: SlidingWindowLimiter::new(10, std::time::Duration::from_secs(60)),
			music: SlidingWindowLimiter::new(15, std::time::Duration::from_secs(30)),
			moderation: SlidingWindowLimiter::new(5, std::time::Duration::from_secs(60)),
			stocks: SlidingWindowLimiter::new(10, std::time::Duration::from_secs(30)),
			welcome: SlidingWindowLimiter::new(1, std::time::Duration::from_secs(5)),
		}
	}

	pub fn cleanup_all(&self) {
		self.ai.cleanup();
		self.music.cleanup();
		self.moderation.cleanup();
		self.stocks.cleanup();
		self.welcome.cleanup();
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::time::Duration;

	#[test]
	fn check_below_limit_returns_zero() {
		let lim = SlidingWindowLimiter::new(3, Duration::from_secs(60));
		assert_eq!(lim.check("user1"), 0);
		assert_eq!(lim.check("user1"), 0);
		assert_eq!(lim.check("user1"), 0);
	}

	#[test]
	fn check_at_or_above_limit_returns_cooldown() {
		let lim = SlidingWindowLimiter::new(2, Duration::from_secs(60));
		assert_eq!(lim.check("user1"), 0);
		assert_eq!(lim.check("user1"), 0);
		// Third call exceeds the limit; should return a positive cooldown.
		let wait = lim.check("user1");
		assert!(wait > 0, "expected positive cooldown, got {wait}");
		assert!(
			wait <= 61,
			"cooldown should not exceed window+1, got {wait}"
		);
	}

	#[test]
	fn check_keys_are_isolated() {
		let lim = SlidingWindowLimiter::new(1, Duration::from_secs(60));
		assert_eq!(lim.check("alice"), 0);
		// Alice is rate limited now.
		assert!(lim.check("alice") > 0);
		// Bob is independent.
		assert_eq!(lim.check("bob"), 0);
		assert!(lim.check("bob") > 0);
	}

	#[test]
	fn check_allows_again_after_window_expires() {
		// Short window so the test runs in ~1.1s.
		let lim = SlidingWindowLimiter::new(1, Duration::from_secs(1));
		assert_eq!(lim.check("user1"), 0);
		assert!(lim.check("user1") > 0, "should be limited immediately");

		std::thread::sleep(Duration::from_millis(1_100));

		// After the window has elapsed, the old timestamp is pruned and a new
		// request is allowed.
		assert_eq!(lim.check("user1"), 0);
	}

	#[test]
	fn cleanup_removes_stale_buckets() {
		let lim = SlidingWindowLimiter::new(5, Duration::from_millis(100));
		lim.check("a");
		lim.check("b");
		lim.check("c");
		assert_eq!(lim.buckets.len(), 3);

		std::thread::sleep(Duration::from_millis(200));

		lim.cleanup();
		assert_eq!(
			lim.buckets.len(),
			0,
			"all buckets should be pruned after window expires"
		);
	}

	#[test]
	fn cleanup_keeps_active_buckets() {
		let lim = SlidingWindowLimiter::new(5, Duration::from_secs(60));
		lim.check("active1");
		lim.check("active2");
		lim.cleanup();
		assert_eq!(lim.buckets.len(), 2);
	}

	#[test]
	fn cleanup_all_runs_on_every_limiter() {
		let rates = RateLimiters::new();
		rates.ai.check("u");
		rates.music.check("u");
		rates.moderation.check("u");
		rates.stocks.check("u");
		rates.welcome.check("u");
		// Just ensure the aggregate cleanup doesn't panic and leaves active buckets intact.
		rates.cleanup_all();
		assert_eq!(rates.ai.buckets.len(), 1);
		assert_eq!(rates.music.buckets.len(), 1);
	}
}
