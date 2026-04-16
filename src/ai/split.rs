const DISCORD_MAX_LENGTH: usize = 2000;

/// Find the largest byte index <= `pos` that is a valid char boundary.
fn floor_char_boundary(s: &str, pos: usize) -> usize {
	if pos >= s.len() {
		return s.len();
	}
	let mut i = pos;
	while i > 0 && !s.is_char_boundary(i) {
		i -= 1;
	}
	i
}

/// Safely slice a string at `pos`, snapping to the nearest char boundary.
fn safe_split_at(s: &str, pos: usize) -> (&str, &str) {
	let boundary = floor_char_boundary(s, pos);
	(&s[..boundary], &s[boundary..])
}

/// Split a response into chunks that fit within Discord's message limit,
/// preserving code blocks across splits.
pub fn split_response(text: &str) -> Vec<String> {
	if text.len() <= DISCORD_MAX_LENGTH {
		return vec![text.to_string()];
	}

	let mut chunks = Vec::new();
	let mut remaining = text.to_string();

	while !remaining.is_empty() {
		if remaining.len() <= DISCORD_MAX_LENGTH {
			chunks.push(remaining);
			break;
		}

		let safe_end = floor_char_boundary(&remaining, DISCORD_MAX_LENGTH);
		let (candidate, _) = safe_split_at(&remaining, safe_end);
		let code_block_count = candidate.matches("```").count();
		let inside_code_block = code_block_count % 2 == 1;

		if inside_code_block {
			// Find the opening fence of the unclosed code block
			let mut count = 0;
			let mut open_idx = 0;
			let mut i = 0;
			while i < candidate.len() {
				if candidate[i..].starts_with("```") {
					count += 1;
					if count == code_block_count {
						open_idx = i;
						break;
					}
					i += 3;
				} else {
					i += candidate[i..].chars().next().map_or(1, |c| c.len_utf8());
				}
			}

			if open_idx > 200 {
				// Split right before the code block opens
				let split_at = remaining[..open_idx]
					.rfind('\n')
					.filter(|&p| p > 200)
					.unwrap_or(open_idx);
				let split_at = floor_char_boundary(&remaining, split_at);
				let chunk = remaining[..split_at].trim_end().to_string();
				chunks.push(chunk);
				remaining = remaining[split_at..].trim_start().to_string();
			} else {
				// Code block starts early — close it and re-open in next chunk
				let max = floor_char_boundary(&remaining, DISCORD_MAX_LENGTH.saturating_sub(4));
				let split_at = remaining[..max]
					.rfind('\n')
					.filter(|&p| p > 200)
					.unwrap_or(max);
				let split_at = floor_char_boundary(&remaining, split_at);

				// Find the language tag from the opening fence
				let lang = remaining[open_idx..]
					.strip_prefix("```")
					.and_then(|s| s.split('\n').next())
					.map(|s| s.trim())
					.filter(|s| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric()))
					.unwrap_or("");

				let chunk = format!("{}\n```", remaining[..split_at].trim_end());
				chunks.push(chunk);
				remaining = format!("```{lang}\n{}", remaining[split_at..].trim_start());
			}
		} else {
			// Not inside a code block — split on paragraph/line/sentence boundaries
			let split_at = remaining[..safe_end]
				.rfind("\n\n")
				.filter(|&p| p > 200)
				.or_else(|| remaining[..safe_end].rfind('\n').filter(|&p| p > 200))
				.or_else(|| remaining[..safe_end].rfind(". ").filter(|&p| p > 200))
				.unwrap_or(safe_end);

			let end = if split_at < safe_end {
				split_at + 1
			} else {
				split_at
			};
			let end = floor_char_boundary(&remaining, end);
			let chunk = remaining[..end].trim_end().to_string();
			chunks.push(chunk);
			remaining = remaining[end..].trim_start().to_string();
		}
	}

	chunks
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn short_message_passes_through_unchanged() {
		let s = "hello world";
		let chunks = split_response(s);
		assert_eq!(chunks, vec!["hello world".to_string()]);
	}

	#[test]
	fn empty_string_yields_single_empty_chunk() {
		let chunks = split_response("");
		assert_eq!(chunks, vec!["".to_string()]);
	}

	#[test]
	fn message_at_exactly_limit_is_one_chunk() {
		let s = "a".repeat(DISCORD_MAX_LENGTH);
		let chunks = split_response(&s);
		assert_eq!(chunks.len(), 1);
		assert_eq!(chunks[0].len(), DISCORD_MAX_LENGTH);
	}

	#[test]
	fn long_message_splits_into_multiple_chunks_within_limit() {
		// 3 paragraphs of 900 chars each, joined by blank lines → ~2700 chars.
		let para = "x".repeat(900);
		let s = format!("{para}\n\n{para}\n\n{para}");
		let chunks = split_response(&s);
		assert!(
			chunks.len() >= 2,
			"expected splitting; got {}",
			chunks.len()
		);
		for c in &chunks {
			assert!(
				c.len() <= DISCORD_MAX_LENGTH,
				"chunk exceeds limit: {} > {DISCORD_MAX_LENGTH}",
				c.len()
			);
		}
	}

	#[test]
	fn split_prefers_paragraph_boundary() {
		// Long paragraph followed by a paragraph break inside the first 2000 chars.
		let first = "a".repeat(1500);
		let second = "b".repeat(1500);
		let s = format!("{first}\n\n{second}");
		let chunks = split_response(&s);
		assert!(chunks.len() >= 2);
		// First chunk should end with the first paragraph (no `b`s in it).
		assert!(
			!chunks[0].contains('b'),
			"first chunk leaked into paragraph 2"
		);
	}

	#[test]
	fn code_block_split_produces_balanced_fences() {
		// Build a single code block that's clearly larger than 2000 chars so
		// the splitter has to break it. The implementation closes the open
		// fence in the first chunk and re-opens in the second.
		let lines: Vec<String> = (0..200).map(|i| format!("line {i:04}")).collect();
		let body = lines.join("\n");
		let s = format!("intro paragraph\n\n```rust\n{body}\n```");
		let chunks = split_response(&s);
		assert!(chunks.len() >= 2, "expected at least 2 chunks");
		for c in &chunks {
			assert!(
				c.len() <= DISCORD_MAX_LENGTH,
				"chunk too large: {}",
				c.len()
			);
			let count = c.matches("```").count();
			assert!(
				count.is_multiple_of(2),
				"chunk has unbalanced ``` fences ({count}): {c:?}",
			);
		}
	}

	#[test]
	fn multibyte_utf8_does_not_panic() {
		// String of 4-byte chars repeated — total bytes >>> 2000 so a naive byte
		// slice at index 2000 would split a char and panic. The splitter must
		// snap to a char boundary.
		let chunk = "𝓗𝓮𝓵𝓵𝓸"; // 5 chars × 4 bytes = 20 bytes per repetition
		let s = chunk.repeat(300); // ~6000 bytes
		let chunks = split_response(&s);
		// Each chunk reassembled by concatenation must equal the original.
		let joined: String = chunks.join("");
		// Splitter can drop whitespace at boundaries, but here there's no
		// whitespace at all so the join must be lossless.
		assert_eq!(
			joined.len(),
			s.len(),
			"byte length differs after split/join"
		);
		for c in &chunks {
			assert!(c.len() <= DISCORD_MAX_LENGTH);
			// Should be valid UTF-8 (any &str slice is by construction).
			let _ = c.chars().count();
		}
	}

	#[test]
	fn floor_char_boundary_snaps_to_valid_index() {
		// Byte index 1 of "𝓗" is mid-char; floor must return 0.
		let s = "𝓗ello";
		assert_eq!(floor_char_boundary(s, 0), 0);
		assert_eq!(floor_char_boundary(s, 1), 0);
		assert_eq!(floor_char_boundary(s, 3), 0);
		assert_eq!(floor_char_boundary(s, 4), 4); // start of 'e'
		assert_eq!(floor_char_boundary(s, s.len() + 100), s.len());
	}

	#[test]
	fn safe_split_at_returns_str_pair() {
		let (a, b) = safe_split_at("hello world", 5);
		assert_eq!(a, "hello");
		assert_eq!(b, " world");
	}

	#[test]
	fn very_long_no_breaks_still_splits() {
		// 5000 chars of one giant word — no spaces, no newlines, no fences.
		// Splitter falls back to safe_end (char boundary) and must still split
		// without panicking and without producing oversized chunks.
		let s = "x".repeat(5000);
		let chunks = split_response(&s);
		assert!(chunks.len() >= 2);
		for c in &chunks {
			assert!(c.len() <= DISCORD_MAX_LENGTH);
		}
		assert_eq!(chunks.iter().map(|c| c.len()).sum::<usize>(), 5000);
	}
}
