//! Client for the Cookieless Audiences API.
//!
//! ```no_run
//! # async fn run() -> Result<(), cookielessaudiences::Error> {
//! let client = cookielessaudiences::Client::new("your_api_key");
//! let result = client.segment("https://example.com/blog").await?;
//! println!("{}", result["audience_type"]);
//! # Ok(()) }
//! ```

use serde_json::Value;
use std::time::Duration;

const BASE: &str = "https://www.cookielessaudiences.com";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("API status {status}: {message}")]
    Api { status: i64, message: &'static str, body: Value },
}

fn status_text(status: i64) -> &'static str {
    match status {
        400 => "bad request, check the parameters",
        401 => "invalid API key",
        403 => "key not active or monthly credits used up",
        407 => "missing data_type, must be url or text",
        410 => "not enough content in the page or text",
        411 => "the URL content could not be fetched",
        500 => "general error, check the request or contact support",
        _ => "API error",
    }
}

pub struct Client {
    api_key: String,
    http: reqwest::Client,
}

impl Client {
    pub fn new(api_key: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .user_agent(concat!("cookielessaudiences-rust/", env!("CARGO_PKG_VERSION"), " (+https://www.cookielessaudiences.com)"))
            .build()
            .expect("http client");
        Client { api_key: api_key.into(), http }
    }

    async fn post(&self, path: &str, form: &[(&str, &str)]) -> Result<Value, Error> {
        let body: Value = self.http.post(format!("{BASE}{path}")).form(form).send().await?.json().await?;
        let status = body.get("status").and_then(Value::as_i64).unwrap_or(200);
        if status != 200 {
            return Err(Error::Api { status, message: status_text(status), body });
        }
        Ok(body)
    }

    /// Page-level audience segmentation in the structured v2 shape.
    pub async fn segment(&self, url: &str) -> Result<Value, Error> {
        self.post("/api/audience/segment.php", &[("query", url), ("api_key", &self.api_key), ("format", "structured")]).await
    }

    /// Legacy free-text v1 shape.
    pub async fn segment_legacy(&self, url: &str) -> Result<Value, Error> {
        self.post("/api/audience/segment.php", &[("query", url), ("api_key", &self.api_key)]).await
    }

    /// IAB content categorization of a URL with confidence scores.
    pub async fn categorize(&self, url: &str) -> Result<Value, Error> {
        self.post(
            "/api/iab/iab_web_content_filtering.php",
            &[("query", url), ("api_key", &self.api_key), ("data_type", "url"), ("confidence", "1")],
        )
        .await
    }

    /// IAB content categorization of plain text.
    pub async fn categorize_text(&self, text: &str) -> Result<Value, Error> {
        self.post(
            "/api/iab/iab_content_filtering.php",
            &[("query", text), ("api_key", &self.api_key), ("data_type", "text"), ("confidence", "1")],
        )
        .await
    }

    /// Public vocabularies, no key needed.
    pub async fn vocabularies() -> Result<Value, Error> {
        Ok(reqwest::get(format!("{BASE}/api/audience/filters.php")).await?.json().await?)
    }
}

/// Readable labels for the INT.* and PI.* codes of a structured response.
pub fn labels_for(result: &Value) -> Vec<String> {
    let names = result.get("labels");
    let mut out = Vec::new();
    for group in ["interests", "purchase_intent"] {
        if let Some(block) = result.get(group) {
            for key in ["tier1", "tier2", "codes"] {
                if let Some(list) = block.get(key).and_then(Value::as_array) {
                    for code in list.iter().filter_map(Value::as_str) {
                        let label = names.and_then(|n| n.get(code)).and_then(Value::as_str).unwrap_or(code);
                        out.push(label.to_string());
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn labels_resolve() {
        let r = json!({"interests":{"tier1":["INT.a"]},"purchase_intent":{"codes":["PI.b","PI.c"]},"labels":{"INT.a":"A","PI.b":"B"}});
        assert_eq!(labels_for(&r), vec!["A", "B", "PI.c"]);
    }
}
