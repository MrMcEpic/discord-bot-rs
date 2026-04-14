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
