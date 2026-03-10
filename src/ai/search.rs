use regex::Regex;
use std::sync::LazyLock;

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
    client: &reqwest::Client,
    query: &str,
    max_results: usize,
) -> Result<Vec<SearchResult>, String> {
    let params = [("q", query)];

    let response = client
        .post("https://html.duckduckgo.com/html/")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("User-Agent", "Mozilla/5.0")
        .form(&params)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Search request failed: {e}"))?;

    let html = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {e}"))?;

    Ok(parse_results(&html, max_results))
}

fn strip_html(s: &str) -> String {
    // Remove HTML tags
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
