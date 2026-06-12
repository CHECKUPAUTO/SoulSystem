use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiType {
    Rest,
    GraphQl,
    Grpc,
    WebSocket,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEndpoint {
    pub url: String,
    pub method: String,
    pub api_type: ApiType,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiTopology {
    pub endpoints: Vec<ApiEndpoint>,
    pub base_url: String,
}

pub struct ApiTopologyScanner {
    client: reqwest::Client,
}

impl ApiTopologyScanner {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    #[instrument(skip(self))]
    pub async fn scan(&self, base_url: &str) -> Result<ApiTopology, String> {
        info!("Scanning API topology for {}", base_url);
        let base = Url::parse(base_url).map_err(|e| e.to_string())?;

        let common_paths = vec!["/api/v1", "/api/v2", "/v1", "/v2", "/graphql", "/ws"];
        let mut tasks = FuturesUnordered::new();

        for path in common_paths {
            let target = base.join(path).map_err(|e| e.to_string())?;
            let client = self.client.clone();
            tasks.push(async move {
                match client.get(target.clone()).send().await {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        if resp.status().is_success() || status == 401 || status == 403 {
                            let api_type = if path.contains("graphql") {
                                ApiType::GraphQl
                            } else if path.contains("ws") {
                                ApiType::WebSocket
                            } else {
                                ApiType::Rest
                            };

                            Some(ApiEndpoint {
                                url: target.to_string(),
                                method: "GET".to_string(),
                                api_type,
                                description: Some(format!("Discovered via common path: {path}")),
                            })
                        } else {
                            None
                        }
                    }
                    Err(_) => None,
                }
            });
        }

        let mut endpoints = Vec::new();
        while let Some(endpoint) = tasks.next().await {
            if let Some(e) = endpoint {
                endpoints.push(e);
            }
        }

        Ok(ApiTopology {
            endpoints,
            base_url: base_url.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_topology_scanner_initialization() {
        let scanner = ApiTopologyScanner::new(reqwest::Client::new());
        let res = scanner.scan("http://localhost").await;
        // Basic check that we can call scan
        assert!(res.is_ok());
    }
}
