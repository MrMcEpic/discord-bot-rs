use regex::Regex;
use std::sync::LazyLock;
use tokio::process::Command;

static LINK_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"class="result__a"[^>]*href="([^"]*)"[^>]*>([\s\S]*?)</a>"#).unwrap()
});

static SNIPPET_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"class="result__snippet"[^>]*>([\s\S]*?)</a>"#).unwrap()
});

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

pub async fn web_search(
    _client: &reqwest::Client,
    query: &str,
    max_results: usize,
) -> Result<Vec<SearchResult>, String> {
    let form_data = format!("q={}", urlencoding::encode(query));

    // Shell out to curl (matches the working TS implementation exactly).
    // reqwest sends extra headers that cause DDG to return a non-result page.
    let output = Command::new("curl")
        .args([
            "-s",
            "-X", "POST",
            "https://html.duckduckgo.com/html/",
            "-H", "Content-Type: application/x-www-form-urlencoded",
            "-H", "User-Agent: Mozilla/5.0",
            "-d", &form_data,
            "--max-time", "10",
        ])
        .output()
        .await
        .map_err(|e| format!("curl failed: {e}"))?;

    if !output.status.success() {
        return Err(format!("curl exited with {}", output.status));
    }

    let html = String::from_utf8_lossy(&output.stdout);

    Ok(parse_results(&html, max_results))
}

fn strip_html(s: &str) -> String {
    let s = Regex::new(r"<[^>]*>").unwrap().replace_all(s, "");
    s.replace("&#x27;", "'")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_results(html: &str, max: usize) -> Vec<SearchResult> {
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
