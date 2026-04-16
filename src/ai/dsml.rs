use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

static DSML_INVOKE: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(
		r#"<\x{ff5c}DSML\x{ff5c}invoke\s+name="([^"]+)"[^>]*>([\s\S]*?)<\x{ff5c}DSML\x{ff5c}/invoke>"#,
	)
	.unwrap()
});

static DSML_PARAM: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r#"<\x{ff5c}DSML\x{ff5c}parameter\s+name="([^"]+)"[^>]*>([\s\S]*?)<\x{ff5c}DSML\x{ff5c}/?parameter>"#).unwrap()
});

pub struct DsmlToolCall {
	pub name: String,
	pub arguments: HashMap<String, String>,
}

/// Parse DSML tool-call blocks from text. Returns extracted calls and cleaned content.
pub fn parse_dsml(content: &str) -> (Vec<DsmlToolCall>, String) {
	let mut calls = Vec::new();

	for cap in DSML_INVOKE.captures_iter(content) {
		let name = cap[1].to_string();
		let body = &cap[2];
		let mut args = HashMap::new();

		for param_cap in DSML_PARAM.captures_iter(body) {
			args.insert(param_cap[1].to_string(), param_cap[2].trim().to_string());
		}

		calls.push(DsmlToolCall {
			name,
			arguments: args,
		});
	}

	// Strip only the matched tool blocks, not from-here-to-EOF. Run the
	// invoke pass first (it consumes nested parameter blocks), then a
	// param pass to clean up any stray parameter blocks the model emitted
	// outside an invoke.
	let stripped = DSML_INVOKE.replace_all(content, "");
	let stripped = DSML_PARAM.replace_all(&stripped, "");
	let cleaned = stripped.trim().to_string();

	(calls, cleaned)
}

#[cfg(test)]
mod tests {
	use super::*;

	// Fullwidth vertical bar — the DSML delimiter character.
	const B: char = '\u{ff5c}';

	/// Build an invoke block like `<｜DSML｜invoke name="NAME">BODY<｜DSML｜/invoke>`.
	fn invoke(name: &str, body: &str) -> String {
		format!("<{B}DSML{B}invoke name=\"{name}\">{body}<{B}DSML{B}/invoke>")
	}

	/// Build a parameter block with a closing `/parameter` tag.
	fn param_slash(name: &str, value: &str) -> String {
		format!("<{B}DSML{B}parameter name=\"{name}\">{value}<{B}DSML{B}/parameter>")
	}

	/// Build a parameter block with no slash on the close (the regression case).
	fn param_no_slash(name: &str, value: &str) -> String {
		format!("<{B}DSML{B}parameter name=\"{name}\">{value}<{B}DSML{B}parameter>")
	}

	#[test]
	fn parses_single_invoke_with_one_param() {
		let text = invoke("search", &param_slash("query", "hello"));
		let (calls, cleaned) = parse_dsml(&text);
		assert_eq!(calls.len(), 1);
		assert_eq!(calls[0].name, "search");
		assert_eq!(calls[0].arguments.get("query"), Some(&"hello".to_string()));
		assert_eq!(cleaned, "");
	}

	#[test]
	fn parses_invoke_with_multiple_params() {
		let body = format!(
			"{}{}",
			param_slash("query", "cats"),
			param_slash("limit", "5")
		);
		let text = invoke("search", &body);
		let (calls, _) = parse_dsml(&text);
		assert_eq!(calls.len(), 1);
		assert_eq!(calls[0].arguments.len(), 2);
		assert_eq!(calls[0].arguments.get("query"), Some(&"cats".to_string()));
		assert_eq!(calls[0].arguments.get("limit"), Some(&"5".to_string()));
	}

	#[test]
	fn parses_param_close_with_optional_slash() {
		// Regression: close tag is optional-slash `/?parameter`. Both variants
		// must parse.
		let text_with = invoke("t", &param_slash("k", "v1"));
		let text_without = invoke("t", &param_no_slash("k", "v2"));

		let (c1, _) = parse_dsml(&text_with);
		let (c2, _) = parse_dsml(&text_without);
		assert_eq!(c1[0].arguments.get("k"), Some(&"v1".to_string()));
		assert_eq!(c2[0].arguments.get("k"), Some(&"v2".to_string()));
	}

	#[test]
	fn malformed_no_closing_invoke_returns_no_calls() {
		let text = format!(
			"<{B}DSML{B}invoke name=\"search\">{}",
			param_slash("query", "hello")
		);
		let (calls, cleaned) = parse_dsml(&text);
		assert_eq!(calls.len(), 0);
		// The param regex still strips the inner param, so cleaned keeps the
		// orphan opening tag but the parameter block is removed.
		assert!(cleaned.contains("invoke name=\"search\""));
		assert!(!cleaned.contains("parameter name"));
	}

	#[test]
	fn parses_multiple_invokes() {
		let text = format!(
			"{} then {}",
			invoke("a", &param_slash("x", "1")),
			invoke("b", &param_slash("y", "2"))
		);
		let (calls, cleaned) = parse_dsml(&text);
		assert_eq!(calls.len(), 2);
		assert_eq!(calls[0].name, "a");
		assert_eq!(calls[1].name, "b");
		// Prose between the invokes should be preserved.
		assert_eq!(cleaned, "then");
	}

	#[test]
	fn prose_around_invokes_is_preserved() {
		let text = format!(
			"Before text {} after text",
			invoke("search", &param_slash("q", "v"))
		);
		let (calls, cleaned) = parse_dsml(&text);
		assert_eq!(calls.len(), 1);
		assert_eq!(cleaned, "Before text  after text");
	}

	#[test]
	fn empty_input_returns_empty() {
		let (calls, cleaned) = parse_dsml("");
		assert!(calls.is_empty());
		assert_eq!(cleaned, "");
	}

	#[test]
	fn no_dsml_content_passes_through() {
		let text = "Just some plain prose with no tool calls.";
		let (calls, cleaned) = parse_dsml(text);
		assert!(calls.is_empty());
		assert_eq!(cleaned, text);
	}

	#[test]
	fn param_values_are_trimmed() {
		let text = invoke("t", &param_slash("k", "  spaced  "));
		let (calls, _) = parse_dsml(&text);
		assert_eq!(calls[0].arguments.get("k"), Some(&"spaced".to_string()));
	}

	#[test]
	fn stray_param_outside_invoke_is_stripped() {
		let text = format!("prose {} more", param_slash("k", "v"));
		let (calls, cleaned) = parse_dsml(&text);
		// No invoke wrapper means no call is recorded.
		assert!(calls.is_empty());
		// But the parameter block is still stripped from cleaned output.
		assert_eq!(cleaned, "prose  more");
	}
}
