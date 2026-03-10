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
}

impl RateLimiters {
    pub fn new() -> Self {
        Self {
            ai: SlidingWindowLimiter::new(10, std::time::Duration::from_secs(60)),
            music: SlidingWindowLimiter::new(15, std::time::Duration::from_secs(30)),
            moderation: SlidingWindowLimiter::new(5, std::time::Duration::from_secs(60)),
        }
    }

    pub fn cleanup_all(&self) {
        self.ai.cleanup();
        self.music.cleanup();
        self.moderation.cleanup();
    }
}
