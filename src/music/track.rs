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

pub async fn resolve_track(query: &str, requested_by: &str) -> Result<Track, String> {
    let tracks = resolve_tracks(query, requested_by, true).await?;
    tracks
        .into_iter()
        .next()
        .ok_or_else(|| "No tracks found".to_string())
}

pub async fn resolve_tracks(
    query: &str,
    requested_by: &str,
    single_only: bool,
) -> Result<Vec<Track>, String> {
    let is_url = query.starts_with("http://") || query.starts_with("https://");
    let cookies = cookies_path();

    let mut args = vec![
        "--dump-json".to_string(),
        "--no-download".to_string(),
        "--js-runtimes".to_string(),
        node_runtime(),
        "--cookies".to_string(),
        cookies,
        "--remote-components".to_string(),
        "ejs:github".to_string(),
        "-f".to_string(),
        "bestaudio".to_string(),
    ];

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

    let output = Command::new("yt-dlp")
        .args(&args)
        .env_remove("NODE_CHANNEL_FD")
        .env_remove("NODE_CHANNEL_SERIALIZATION_MODE")
        .output()
        .await
        .map_err(|e| format!("Failed to spawn yt-dlp: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("yt-dlp failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let tracks: Vec<Track> = stdout
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
        .collect();

    Ok(tracks)
}

