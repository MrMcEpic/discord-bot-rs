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
	Regex::new(r#"<\x{ff5c}DSML\x{ff5c}parameter\s+name="([^"]+)"[^>]*>([\s\S]*?)<\x{ff5c}DSML\x{ff5c}parameter>"#).unwrap()
});

static DSML_STRIP: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"<\x{ff5c}DSML\x{ff5c}[\s\S]*$").unwrap());

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

	let cleaned = DSML_STRIP.replace(content, "").trim().to_string();

	(calls, cleaned)
}
