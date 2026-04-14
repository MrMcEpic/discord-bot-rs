use regex::Regex;
use std::sync::LazyLock;

static ROLE_MARKER: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"(?im)^(system|assistant|user)\s*:").unwrap());
static DEEPSEEK_TOKEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<\|.*?\|>").unwrap());
static LLAMA_INST: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\[/?INST\]").unwrap());
static LLAMA_SYS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<<?/?SYS>>?").unwrap());

pub fn sanitize_content(text: &str) -> String {
	let s = ROLE_MARKER.replace_all(text, "[$1]:").to_string();
	let s = DEEPSEEK_TOKEN.replace_all(&s, "").to_string();
	let s = LLAMA_INST.replace_all(&s, "").to_string();
	let s = LLAMA_SYS.replace_all(&s, "").to_string();
	s
}
