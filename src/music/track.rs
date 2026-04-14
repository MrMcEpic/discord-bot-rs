use serde::Deserialize;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct Track {
	pub url: String,
	pub title: String,
	pub duration: u64,
	pub thumbnail: String,
	pub requested_by: String,
}

#[derive(Deserialize)]
struct YtDlpJson {
	webpage_url: Option<String>,
	url: Option<String>,
	title: Option<String>,
	duration: Option<f64>,
	thumbnail: Option<String>,
}

/// Resolves the node runtime specifier for yt-dlp's `--js-runtimes` flag.
/// nvm-installed node isn't on PATH for non-login shells, so we pass the full path.
fn node_runtime() -> String {
	let nvm_node = "/home/webapps/.nvm/versions/node/v20.20.1/bin/node";
	if std::path::Path::new(nvm_node).exists() {
		format!("node:{nvm_node}")
	} else {
		"node".to_string()
	}
}

/// Returns yt-dlp args for songbird's YoutubeDl input (cookies, node runtime, etc).
pub fn ytdlp_user_args() -> Vec<String> {
	vec![
		"--cookies".to_string(),
		cookies_path(),
		"--js-runtimes".to_string(),
		node_runtime(),
		"--remote-components".to_string(),
		"ejs:github".to_string(),
		"--no-playlist".to_string(),
	]
}

fn cookies_path() -> String {
	let exe_dir = std::env::current_dir().unwrap_or_default();
	let candidate = exe_dir.join("cookies.txt");
	if candidate.exists() {
		return candidate.to_string_lossy().to_string();
	}
	"cookies.txt".to_string()
}

pub async fn resolve_track(query: &str, requested_by: &str) -> Result<(Track, bool), String> {
	let (tracks, cookies_stale) = resolve_tracks(query, requested_by, true).await?;
	let track = tracks
		.into_iter()
		.next()
		.ok_or_else(|| "No tracks found".to_string())?;
	Ok((track, cookies_stale))
}

/// Returns true if yt-dlp stderr indicates a cookie/auth problem.
fn is_cookie_error(stderr: &str) -> bool {
	let lower = stderr.to_lowercase();
	lower.contains("page needs to be reloaded")
		|| lower.contains("sign in to confirm")
		|| lower.contains("this helps protect our community")
		|| lower.contains("login required")
}

/// Build the yt-dlp argument list.  When `use_cookies` is false the
/// `--cookies` flag is omitted entirely so stale cookies can't break search.
fn build_ytdlp_args(query: &str, single_only: bool, use_cookies: bool) -> Vec<String> {
	let is_url = query.starts_with("http://") || query.starts_with("https://");

	let mut args = vec![
		"--dump-json".to_string(),
		"--no-download".to_string(),
		"--js-runtimes".to_string(),
		node_runtime(),
	];

	if use_cookies {
		let cookies = cookies_path();
		// Only include --cookies if the file actually exists
		if std::path::Path::new(&cookies).exists() {
			args.push("--cookies".to_string());
			args.push(cookies);
		}
	}

	args.extend_from_slice(&[
		"--remote-components".to_string(),
		"ejs:github".to_string(),
		"-f".to_string(),
		"bestaudio".to_string(),
	]);

	if single_only {
		args.push("--no-playlist".to_string());
	} else {
		args.push("--flat-playlist".to_string());
	}

	if is_url {
		args.push(query.to_string());
	} else {
		args.push(format!("ytsearch1:{query}"));
	}

	args
}

fn parse_tracks(stdout: &str, requested_by: &str) -> Vec<Track> {
	stdout
		.lines()
		.filter(|line| !line.is_empty())
		.filter_map(|line| {
			let json: YtDlpJson = serde_json::from_str(line).ok()?;
			Some(Track {
				url: json.webpage_url.or(json.url).unwrap_or_default(),
				title: json.title.unwrap_or_else(|| "Unknown".to_string()),
				duration: json.duration.unwrap_or(0.0) as u64,
				thumbnail: json.thumbnail.unwrap_or_default(),
				requested_by: requested_by.to_string(),
			})
		})
		.collect()
}

async fn run_ytdlp(args: &[String]) -> Result<std::process::Output, String> {
	Command::new("yt-dlp")
		.args(args)
		.env_remove("NODE_CHANNEL_FD")
		.env_remove("NODE_CHANNEL_SERIALIZATION_MODE")
		.output()
		.await
		.map_err(|e| format!("Failed to spawn yt-dlp: {e}"))
}

/// Resolves tracks from a query or URL.
/// Returns `(tracks, cookies_stale)` — when `cookies_stale` is true the caller
/// should warn the user that cookies need refreshing.
pub async fn resolve_tracks(
	query: &str,
	requested_by: &str,
	single_only: bool,
) -> Result<(Vec<Track>, bool), String> {
	// First attempt: with cookies
	let args = build_ytdlp_args(query, single_only, true);
	let output = run_ytdlp(&args).await?;

	if output.status.success() {
		let stdout = String::from_utf8_lossy(&output.stdout);
		return Ok((parse_tracks(&stdout, requested_by), false));
	}

	// Check if the failure is cookie-related
	let stderr = String::from_utf8_lossy(&output.stderr);
	if !is_cookie_error(&stderr) {
		return Err(format!("yt-dlp failed: {stderr}"));
	}

	// Retry without cookies
	tracing::warn!("yt-dlp cookie error, retrying without cookies: {stderr}");
	let args = build_ytdlp_args(query, single_only, false);
	let output = run_ytdlp(&args).await?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		return Err(format!("yt-dlp failed: {stderr}"));
	}

	let stdout = String::from_utf8_lossy(&output.stdout);
	Ok((parse_tracks(&stdout, requested_by), true))
}
