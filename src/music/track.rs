use serde::Deserialize;
use std::process::Child;
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
        "node".to_string(),
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

/// Spawn yt-dlp | ffmpeg pipeline, returning both child processes as a Vec<Child>.
/// The last child (ffmpeg) must have stdout piped for songbird ChildContainer to read.
pub struct AudioPipeline {
    ytdlp: Option<Child>,
    ffmpeg: Option<Child>,
}

impl AudioPipeline {
    /// Spawn the pipeline. Returns AudioPipeline (for cleanup) and the unused stdout
    /// (which ChildContainer will read via the ffmpeg child).
    ///
    /// NOTE: For ChildContainer, we need the ffmpeg child to still have stdout attached.
    /// So we return the children directly.
    pub fn spawn(url: &str) -> Result<Self, String> {
        let cookies = cookies_path();

        let mut ytdlp = std::process::Command::new("yt-dlp")
            .args([
                "-f",
                "bestaudio",
                "--no-playlist",
                "--js-runtimes",
                "node",
                "--cookies",
                &cookies,
                "--remote-components",
                "ejs:github",
                "-o",
                "-",
                url,
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn yt-dlp: {e}"))?;

        let ytdlp_stdout = ytdlp.stdout.take().ok_or("No yt-dlp stdout")?;

        let ffmpeg = std::process::Command::new("ffmpeg")
            .args([
                "-i",
                "pipe:0",
                "-analyzeduration",
                "0",
                "-loglevel",
                "0",
                "-c:a",
                "libopus",
                "-b:a",
                "256k",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-application",
                "audio",
                "-f",
                "ogg",
                "pipe:1",
            ])
            .stdin(ytdlp_stdout)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn ffmpeg: {e}"))?;

        Ok(Self { ytdlp: Some(ytdlp), ffmpeg: Some(ffmpeg) })
    }

    /// Consume into a Vec<Child> for songbird's ChildContainer.
    /// ChildContainer reads stdout from the LAST child in the vec.
    pub fn into_children(&mut self) -> Vec<Child> {
        let mut children = Vec::new();
        if let Some(ytdlp) = self.ytdlp.take() {
            children.push(ytdlp);
        }
        if let Some(ffmpeg) = self.ffmpeg.take() {
            children.push(ffmpeg);
        }
        children
    }

    pub fn kill(&mut self) {
        if let Some(ref mut ytdlp) = self.ytdlp {
            let _ = ytdlp.kill();
        }
        if let Some(ref mut ffmpeg) = self.ffmpeg {
            let _ = ffmpeg.kill();
        }
    }
}

impl Drop for AudioPipeline {
    fn drop(&mut self) {
        // Only kill children that haven't been taken via into_children()
        self.kill();
    }
}
