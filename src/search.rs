use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

const ES_USER: &str = "aWVSALXpZv";
const ES_PASS: &str = "X8gPHnzL52wFEekuxsfQ9cSh";
const DEFAULT_SCHEMA: &str = "48";
const DEFAULT_CHANNEL: &str = "unstable";

#[derive(Debug, Clone)]
pub struct Package {
    pub attr_name: String,
    pub version: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
struct EsResponse {
    hits: EsHits,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct EsHits {
    hits: Vec<EsHit>,
    total: EsTotal,
}

#[derive(Debug, Deserialize)]
struct EsTotal {
    value: u64,
}

#[derive(Debug, Deserialize)]
struct EsHit {
    #[serde(rename = "_source")]
    source: EsSource,
}

#[derive(Debug, Deserialize)]
struct EsSource {
    package_attr_name: String,
    #[serde(default)]
    package_pversion: Option<String>,
    #[serde(default)]
    package_description: Option<String>,
}

pub struct SearchClient {
    http: reqwest::Client,
    schema: String,
    channel: String,
}

impl SearchClient {
    pub fn new(channel: Option<String>) -> Self {
        let schema = load_cached_schema().unwrap_or_else(|| DEFAULT_SCHEMA.to_string());
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("http client"),
            schema,
            channel: channel.unwrap_or_else(|| DEFAULT_CHANNEL.to_string()),
        }
    }

    pub fn channel(&self) -> &str {
        &self.channel
    }

    pub fn set_channel(&mut self, channel: String) {
        self.channel = channel;
    }

    pub async fn search(
        &mut self,
        query: &str,
        size: usize,
    ) -> Result<(Vec<Package>, Duration, u64)> {
        if query.trim().is_empty() {
            return Ok((Vec::new(), Duration::ZERO, 0));
        }
        match self.search_attempt(query, size).await {
            Ok(r) => Ok(r),
            Err(e) if e.to_string().contains("no such index") => {
                if let Some(schema) = self.resolve_schema().await {
                    self.schema = schema.clone();
                    save_cached_schema(&schema);
                    self.search_attempt(query, size).await
                } else {
                    Err(e)
                }
            }
            Err(e) => Err(e),
        }
    }

    async fn search_attempt(
        &self,
        query: &str,
        size: usize,
    ) -> Result<(Vec<Package>, Duration, u64)> {
        let url = format!(
            "https://search.nixos.org/backend/latest-{}-nixos-{}/_search",
            self.schema, self.channel
        );
        let payload = build_query(query, size);
        let start = Instant::now();

        let resp = self
            .http
            .post(&url)
            .basic_auth(ES_USER, Some(ES_PASS))
            .header("Content-Type", "application/json")
            .body(payload)
            .send()
            .await
            .context("request failed")?;

        let body = resp.text().await.context("read body")?;
        let elapsed = start.elapsed();
        let es: EsResponse = serde_json::from_str(&body).context("parse response")?;

        if let Some(err) = es.error {
            let reason = err
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("elasticsearch error");
            return Err(anyhow!("{reason}"));
        }

        let total = es.hits.total.value;
        let packages = es
            .hits
            .hits
            .into_iter()
            .map(|h| Package {
                attr_name: h.source.package_attr_name,
                version: h.source.package_pversion.unwrap_or_default(),
                description: h.source.package_description.unwrap_or_default(),
            })
            .collect();

        Ok((packages, elapsed, total))
    }

    async fn resolve_schema(&self) -> Option<String> {
        let cur: i32 = self.schema.parse().ok()?;
        for k in (1..=5).rev() {
            let cand = (cur + k).to_string();
            if self.probe_schema(&cand).await {
                return Some(cand);
            }
        }
        for k in 1..=5 {
            let n = cur - k;
            if n < 1 {
                break;
            }
            let cand = n.to_string();
            if self.probe_schema(&cand).await {
                return Some(cand);
            }
        }
        None
    }

    async fn probe_schema(&self, schema: &str) -> bool {
        let url = format!(
            "https://search.nixos.org/backend/latest-{}-nixos-{}",
            schema, self.channel
        );
        self.http
            .head(&url)
            .basic_auth(ES_USER, Some(ES_PASS))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}

fn build_query(query: &str, size: usize) -> String {
    serde_json::json!({
        "from": 0,
        "size": size,
        "query": {
            "bool": {
                "filter": [{ "term": { "type": { "value": "package" } } }],
                "must": [{
                    "dis_max": {
                        "tie_breaker": 0.7,
                        "queries": [
                            {
                                "multi_match": {
                                    "type": "cross_fields",
                                    "query": query,
                                    "analyzer": "whitespace",
                                    "operator": "and",
                                    "fields": [
                                        "package_attr_name^9",
                                        "package_attr_name.*^5.4",
                                        "package_pname^6",
                                        "package_pname.*^3.6",
                                        "package_description^1.3",
                                        "package_longDescription^1"
                                    ]
                                }
                            },
                            {
                                "multi_match": {
                                    "type": "best_fields",
                                    "query": query,
                                    "analyzer": "whitespace",
                                    "operator": "and",
                                    "fields": ["package_programs^7.5"],
                                    "fuzziness": 1
                                }
                            }
                        ]
                    }
                }]
            }
        }
    })
    .to_string()
}

fn schema_cache_path() -> Option<PathBuf> {
    let base = dirs::cache_dir()?;
    Some(base.join("nixpick").join("schema"))
}

fn load_cached_schema() -> Option<String> {
    let path = schema_cache_path()?;
    let s = std::fs::read_to_string(path).ok()?;
    let s = s.trim().to_string();
    s.parse::<u32>().ok()?;
    Some(s)
}

fn save_cached_schema(schema: &str) {
    if let Some(path) = schema_cache_path() {
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = std::fs::write(path, format!("{schema}\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ripgrep_returns_results() {
        let mut c = SearchClient::new(None);
        let r = c.search("ripgrep", 30).await.expect("search ok");
        let (pkgs, _elapsed, total) = r;
        assert!(!pkgs.is_empty(), "expected at least one hit, total={total}");
        assert!(
            pkgs.iter().any(|p| p.attr_name == "ripgrep"),
            "ripgrep should appear in results, got: {:?}",
            pkgs.iter().map(|p| &p.attr_name).collect::<Vec<_>>()
        );
    }
}
