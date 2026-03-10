const DISCORD_MAX_LENGTH: usize = 2000;

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

        let candidate = &remaining[..DISCORD_MAX_LENGTH.min(remaining.len())];
        let code_block_count = candidate.matches("```").count();
        let inside_code_block = code_block_count % 2 == 1;

        let split_at;

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
                    i += 1;
                }
            }

            if open_idx > 200 {
                // Split right before the code block opens
                split_at = remaining[..open_idx]
                    .rfind('\n')
                    .filter(|&p| p > 200)
                    .unwrap_or(open_idx);
                let chunk = remaining[..split_at].trim_end().to_string();
                chunks.push(chunk);
                remaining = remaining[split_at..].trim_start().to_string();
            } else {
                // Code block starts early — close it and re-open in next chunk
                let max = DISCORD_MAX_LENGTH.saturating_sub(4);
                split_at = remaining[..max]
                    .rfind('\n')
                    .filter(|&p| p > 200)
                    .unwrap_or(max);

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
            split_at = remaining[..DISCORD_MAX_LENGTH]
                .rfind("\n\n")
                .filter(|&p| p > 200)
                .or_else(|| {
                    remaining[..DISCORD_MAX_LENGTH]
                        .rfind('\n')
                        .filter(|&p| p > 200)
                })
                .or_else(|| {
                    remaining[..DISCORD_MAX_LENGTH]
                        .rfind(". ")
                        .filter(|&p| p > 200)
                })
                .unwrap_or(DISCORD_MAX_LENGTH);

            let chunk = remaining[..split_at + 1].trim_end().to_string();
            chunks.push(chunk);
            remaining = remaining[split_at + 1..].trim_start().to_string();
        }
    }

    chunks
}
