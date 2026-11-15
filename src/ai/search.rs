//! Web search for the AI's `web_search` tool.
//!
//! Backends are tried in the order given by `[search] backends` in the
//! instance config (default: SearXNG, then DuckDuckGo). The first backend
//! that returns results wins. A backend that errors or comes back empty
//! falls through to the next one.
//!
//! - **SearXNG** — a self-hosted metasearch instance queried over its JSON
//!   API. Enabled when `SEARXNG_URL` (env) or `[search] searxng_url` is set.
//!   This is the reliable backend: it aggregates several engines, so one of
//!   them blocking a datacenter IP doesn't take search down. The bundled
//!   docker-compose ships an internal-only instance.
//! - **DuckDuckGo** — scrapes the keyless HTML endpoint. Kept as a
//!   zero-setup fallback, but DuckDuckGo now answers most automated traffic
//!   with a bot challenge (HTTP 202 "anomaly" page), so on its own it
//!   frequently returns nothing. That case is reported as an error, not as
//!   "no results", so the model (and the logs) can tell the difference.

use regex::Regex;
use serde::Deserialize;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::process::Command;

static LINK_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r#"class="result__a"[^>]*href="([^"]*)"[^>]*>([\s\S]*?)</a>"#).unwrap()
});

static SNIPPET_PATTERN: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r#"class="result__snippet"[^>]*>([\s\S]*?)</a>"#).unwrap());

static TAG_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").unwrap());

#[derive(Debug, Clone)]
pub struct SearchResult {
	pub title: String,
	pub url: String,
	pub snippet: String,
}

/// One search backend. Names match the strings accepted in
/// `[search] backends`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchBackend {
	Searxng,
	Duckduckgo,
}

impl SearchBackend {
	fn name(self) -> &'static str {
		match self {
			Self::Searxng => "searxng",
			Self::Duckduckgo => "duckduckgo",
		}
	}
}

/// `[search]` section of an instance's `config.toml`. Every field is
/// optional; an absent section means "SearXNG if `SEARXNG_URL` is set, then
/// DuckDuckGo".
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchConfig {
	/// Backends to try, in order.
	#[serde(default = "default_backends")]
	pub backends: Vec<SearchBackend>,
	/// Base URL of a SearXNG instance with the JSON format enabled (e.g.
	/// `http://searxng:8080`). Falls back to the `SEARXNG_URL` env var.
	#[serde(default)]
	pub searxng_url: Option<String>,
	/// Per-backend request timeout.
	#[serde(default = "default_timeout_secs")]
	pub timeout_secs: u64,
}

impl Default for SearchConfig {
	fn default() -> Self {
		Self {
			backends: default_backends(),
			searxng_url: None,
			timeout_secs: default_timeout_secs(),
		}
	}
}

fn default_backends() -> Vec<SearchBackend> {
	vec![SearchBackend::Searxng, SearchBackend::Duckduckgo]
}

fn default_timeout_secs() -> u64 {
	10
}

impl SearchConfig {
	/// Fill `searxng_url` from the environment if the config didn't set it,
	/// and normalise it (no trailing slash, empty = unset). Called once at
	/// startup.
	pub fn resolve(mut self, env_lookup: impl Fn(&str) -> Option<String>) -> Self {
		let url = self
			.searxng_url
			.take()
			.or_else(|| env_lookup("SEARXNG_URL"))
			.map(|u| u.trim().trim_end_matches('/').to_string())
			.filter(|u| !u.is_empty());
		self.searxng_url = url;
		self
	}

	/// The backends that can actually run (SearXNG needs a URL).
	pub fn active_backends(&self) -> Vec<SearchBackend> {
		self.backends
			.iter()
			.copied()
			.filter(|b| *b != SearchBackend::Searxng || self.searxng_url.is_some())
			.collect()
	}
}

/// Run `query` through the configured backends in order.
///
/// Returns the first non-empty result set. `Ok(vec![])` means at least one
/// backend answered cleanly with zero results. `Err` means every backend
/// failed; the message names each backend and why, so it can be passed to
/// the model verbatim.
pub async fn web_search(
	client: &reqwest::Client,
	cfg: &SearchConfig,
	query: &str,
	max_results: usize,
) -> Result<Vec<SearchResult>, String> {
	let backends = cfg.active_backends();
	if backends.is_empty() {
		return Err("no search backend is configured".to_string());
	}
	let timeout = Duration::from_secs(cfg.timeout_secs.max(1));
	let mut errors: Vec<String> = Vec::new();
	let mut any_clean_empty = false;

	for backend in backends {
		let outcome = match backend {
			SearchBackend::Searxng => {
				// active_backends() guarantees the URL is present.
				let base = cfg.searxng_url.as_deref().unwrap_or_default();
				searxng_search(client, base, query, max_results, timeout).await
			}
			SearchBackend::Duckduckgo => ddg_search(query, max_results, timeout).await,
		};
		match outcome {
			Ok(results) if !results.is_empty() => {
				tracing::debug!(
					"Web search via {} returned {} results",
					backend.name(),
					results.len()
				);
				return Ok(results);
			}
			Ok(_) => {
				tracing::info!("Web search via {} returned no results", backend.name());
				any_clean_empty = true;
			}
			Err(e) => {
				tracing::warn!("Web search via {} failed: {e}", backend.name());
				errors.push(format!("{}: {e}", backend.name()));
			}
		}
	}

	if any_clean_empty {
		Ok(Vec::new())
	} else {
		Err(errors.join("; "))
	}
}

// --- SearXNG ---------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SearxResponse {
	#[serde(default)]
	results: Vec<SearxResult>,
	/// `[[engine, reason], ...]` — engines that failed for this query.
	#[serde(default)]
	unresponsive_engines: Vec<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct SearxResult {
	#[serde(default)]
	title: String,
	#[serde(default)]
	url: String,
	#[serde(default)]
	content: Option<String>,
}

async fn searxng_search(
	client: &reqwest::Client,
	base_url: &str,
	query: &str,
	max: usize,
	timeout: Duration,
) -> Result<Vec<SearchResult>, String> {
	let resp = client
		.get(format!("{base_url}/search"))
		.query(&[("q", query), ("format", "json")])
		.timeout(timeout)
		.send()
		.await
		.map_err(describe_reqwest_error)?;
	let status = resp.status();
	if !status.is_success() {
		// 403 here almost always means the instance doesn't have
		// `search.formats: [json]` enabled.
		return Err(format!("HTTP {status}"));
	}
	let body: SearxResponse = resp.json().await.map_err(|e| {
		tracing::debug!("SearXNG JSON decode error: {e}");
		"bad JSON response".to_string()
	})?;
	let results = parse_searx(body.results, max);
	if results.is_empty() && !body.unresponsive_engines.is_empty() {
		let failed: Vec<String> = body
			.unresponsive_engines
			.iter()
			.map(|e| e.join(" "))
			.collect();
		return Err(format!("all engines failed ({})", failed.join(", ")));
	}
	Ok(results)
}

/// Short, URL-free reason for a failed SearXNG request. These strings reach
/// the model (and may be echoed into Discord), so they must not carry the
/// internal base URL or the query string; the full error goes to the debug
/// log only.
fn describe_reqwest_error(e: reqwest::Error) -> String {
	tracing::debug!("SearXNG request error: {e}");
	if e.is_timeout() {
		return "timed out".to_string();
	}
	if e.is_connect() {
		return "unreachable".to_string();
	}
	// The innermost source (e.g. "connection reset by peer") never carries
	// the request URL; reqwest's own Display does.
	let mut root = None;
	let mut cur = std::error::Error::source(&e);
	while let Some(src) = cur {
		root = Some(src.to_string());
		cur = src.source();
	}
	match root {
		Some(r) => format!("request failed ({r})"),
		None => "request failed".to_string(),
	}
}

/// Redact `user:pass@` from a URL before logging it.
pub fn redact_userinfo(url: &str) -> String {
	match url.split_once("://") {
		Some((scheme, rest)) => match rest.split_once('@') {
			Some((creds, host)) if !creds.contains('/') => format!("{scheme}://***@{host}"),
			_ => url.to_string(),
		},
		None => url.to_string(),
	}
}

fn parse_searx(raw: Vec<SearxResult>, max: usize) -> Vec<SearchResult> {
	raw.into_iter()
		.filter(|r| !r.url.is_empty() && !r.title.trim().is_empty())
		.take(max)
		.map(|r| SearchResult {
			title: collapse_ws(&r.title),
			url: r.url,
			snippet: collapse_ws(r.content.as_deref().unwrap_or("")),
		})
		.collect()
}

// --- DuckDuckGo (HTML scrape) ------------------------------------------------

/// Marker for the `-w` status trailer appended to curl's output.
const STATUS_MARKER: &str = "\n__HTTP_STATUS__:";

async fn ddg_search(
	query: &str,
	max_results: usize,
	timeout: Duration,
) -> Result<Vec<SearchResult>, String> {
	let form_data = format!("q={}", urlencoding::encode(query));
	let max_time = timeout.as_secs().to_string();
	let write_out = format!("{STATUS_MARKER}%{{http_code}}");

	// Shell out to curl (matches the original TS implementation): reqwest's
	// default headers got a non-result page from DDG.
	let output = Command::new("curl")
		.args([
			"-s",
			"-X",
			"POST",
			"https://html.duckduckgo.com/html/",
			"-H",
			"Content-Type: application/x-www-form-urlencoded",
			"-H",
			"User-Agent: Mozilla/5.0",
			"-d",
			&form_data,
			"--max-time",
			&max_time,
			"-w",
			&write_out,
		])
		.output()
		.await
		.map_err(|e| format!("curl failed: {e}"))?;

	if !output.status.success() {
		return Err(format!("curl exited with {}", output.status));
	}

	let raw = String::from_utf8_lossy(&output.stdout);
	let (html, status) = split_status(&raw);
	let results = parse_ddg(html, max_results);
	if results.is_empty() {
		if let Some(reason) = ddg_block_reason(html, status) {
			return Err(reason);
		}
	}
	Ok(results)
}

/// Split curl's body from the `-w` status trailer.
fn split_status(raw: &str) -> (&str, Option<u16>) {
	match raw.rfind(STATUS_MARKER) {
		Some(i) => (
			&raw[..i],
			raw[i + STATUS_MARKER.len()..].trim().parse().ok(),
		),
		None => (raw, None),
	}
}

/// Explain an empty DuckDuckGo response that isn't a genuine "no results":
/// the bot-challenge page (served with HTTP 202) or any non-200 status.
fn ddg_block_reason(html: &str, status: Option<u16>) -> Option<String> {
	if html.contains("anomaly-modal") || html.contains("anomaly.js") || status == Some(202) {
		return Some(
			"DuckDuckGo served a bot-detection challenge instead of results (blocked)".to_string(),
		);
	}
	match status {
		Some(200) | None => None,
		Some(code) => Some(format!("DuckDuckGo returned HTTP {code}")),
	}
}

fn strip_html(s: &str) -> String {
	let s = TAG_PATTERN.replace_all(s, "");
	collapse_ws(
		&s.replace("&#x27;", "'")
			.replace("&amp;", "&")
			.replace("&lt;", "<")
			.replace("&gt;", ">")
			.replace("&quot;", "\""),
	)
}

fn collapse_ws(s: &str) -> String {
	s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_ddg(html: &str, max: usize) -> Vec<SearchResult> {
	let mut results = Vec::new();

	let links: Vec<(String, String)> = LINK_PATTERN
		.captures_iter(html)
		.map(|cap| (cap[1].to_string(), strip_html(&cap[2])))
		.collect();

	let snippets: Vec<String> = SNIPPET_PATTERN
		.captures_iter(html)
		.map(|cap| strip_html(&cap[1]))
		.collect();

	for (i, (url, title)) in links.iter().enumerate() {
		if results.len() >= max {
			break;
		}
		if url.is_empty() || title.is_empty() || url.starts_with("https://duckduckgo.com") {
			continue;
		}
		results.push(SearchResult {
			title: title.clone(),
			url: url.clone(),
			snippet: snippets.get(i).cloned().unwrap_or_default(),
		});
	}

	results
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn config_defaults_and_env_fallback() {
		let cfg = SearchConfig::default().resolve(|_| None);
		assert_eq!(cfg.backends, default_backends());
		assert_eq!(cfg.active_backends(), vec![SearchBackend::Duckduckgo]);

		let cfg = SearchConfig::default()
			.resolve(|k| (k == "SEARXNG_URL").then(|| "http://searxng:8080/ ".to_string()));
		assert_eq!(cfg.searxng_url.as_deref(), Some("http://searxng:8080"));
		assert_eq!(
			cfg.active_backends(),
			vec![SearchBackend::Searxng, SearchBackend::Duckduckgo]
		);
	}

	#[test]
	fn config_url_beats_env_and_empty_is_unset() {
		let cfg: SearchConfig =
			toml::from_str("backends = [\"searxng\"]\nsearxng_url = \"http://a:1\"").unwrap();
		let cfg = cfg.resolve(|_| Some("http://env:2".to_string()));
		assert_eq!(cfg.searxng_url.as_deref(), Some("http://a:1"));

		let cfg = SearchConfig::default().resolve(|_| Some("  ".to_string()));
		assert!(cfg.searxng_url.is_none());
	}

	#[test]
	fn config_rejects_unknown_backend() {
		assert!(toml::from_str::<SearchConfig>("backends = [\"bing\"]").is_err());
	}

	#[test]
	fn searx_results_are_cleaned_and_capped() {
		let body: SearxResponse = serde_json::from_str(
			r#"{"results":[
				{"title":"  A\n title ","url":"https://a","content":"snip  pet"},
				{"title":"","url":"https://empty-title"},
				{"title":"B","url":"","content":"no url"},
				{"title":"C","url":"https://c"},
				{"title":"D","url":"https://d"}
			],"unresponsive_engines":[["duckduckgo","CAPTCHA"]]}"#,
		)
		.unwrap();
		let r = parse_searx(body.results, 2);
		assert_eq!(r.len(), 2);
		assert_eq!(r[0].title, "A title");
		assert_eq!(r[0].snippet, "snip pet");
		assert_eq!(r[1].url, "https://c");
		assert_eq!(r[1].snippet, "");
	}

	#[test]
	fn ddg_challenge_page_is_an_error_not_empty() {
		let challenge = r#"<html><div class="anomaly-modal__title">Unfortunately, bots use DuckDuckGo too.</div></html>"#;
		assert!(parse_ddg(challenge, 10).is_empty());
		assert!(ddg_block_reason(challenge, Some(202))
			.unwrap()
			.contains("bot-detection"));
		assert!(ddg_block_reason("<html></html>", Some(202)).is_some());
		assert_eq!(
			ddg_block_reason("<html></html>", Some(503)).as_deref(),
			Some("DuckDuckGo returned HTTP 503")
		);
		// A genuine empty result page is not an error.
		assert!(ddg_block_reason("<html>No results.</html>", Some(200)).is_none());
	}

	#[test]
	fn userinfo_is_redacted_for_logs() {
		assert_eq!(
			redact_userinfo("http://user:pw@searxng:8080"),
			"http://***@searxng:8080"
		);
		assert_eq!(
			redact_userinfo("http://searxng:8080"),
			"http://searxng:8080"
		);
		assert_eq!(
			redact_userinfo("http://h:1/path@x"),
			"http://h:1/path@x",
			"an @ in the path is not userinfo"
		);
	}

	#[tokio::test]
	async fn reqwest_failure_reason_has_no_url_or_query() {
		// Port 9 (discard) on localhost: refused immediately.
		let client = reqwest::Client::new();
		let err = searxng_search(
			&client,
			"http://127.0.0.1:9",
			"secret query",
			5,
			Duration::from_secs(2),
		)
		.await
		.unwrap_err();
		assert!(!err.contains("127.0.0.1"), "{err}");
		assert!(!err.contains("secret"), "{err}");
	}

	#[test]
	fn split_status_reads_curl_trailer() {
		let raw = format!("<html>body</html>{STATUS_MARKER}202");
		assert_eq!(split_status(&raw), ("<html>body</html>", Some(202)));
		assert_eq!(split_status("no trailer"), ("no trailer", None));
	}

	#[test]
	fn ddg_results_parse() {
		let html = r##"<a class="result__a" href="https://example.com/x">Example &amp; Co</a>
			<a class="result__snippet" href="#">The <b>snippet</b></a>
			<a class="result__a" href="https://duckduckgo.com/y">ad</a>"##;
		let r = parse_ddg(html, 10);
		assert_eq!(r.len(), 1);
		assert_eq!(r[0].title, "Example & Co");
		assert_eq!(r[0].snippet, "The snippet");
	}
}
