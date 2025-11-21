use anyhow::{anyhow, Result};
use reqwest::Client;
use std::time::Duration;
use tracing::{error, info};

use super::models::{CloudDeployRequest, CloudDeployResponse};
use crate::config::CloudConfig;

#[derive(Debug, Clone)]
pub struct SkyPilotClient {
    client: Client,
    endpoint: String,
    api_key: String,
}

impl SkyPilotClient {
    pub fn new(config: &CloudConfig) -> Result<Self> {
        let endpoint = config
            .api_endpoint
            .clone()
            .ok_or_else(|| anyhow!("Cloud API endpoint not configured"))?;
        let api_key = config
            .api_key
            .clone()
            .ok_or_else(|| anyhow!("Cloud API key not configured"))?;

        let client = Client::builder()
            .timeout(Duration::from_secs(30)) // Timeout for the initial request
            .build()
            .map_err(|e| anyhow!("Failed to create HTTP client: {}", e))?;

        Ok(Self {
            client,
            endpoint,
            api_key,
        })
    }

    pub async fn deploy(&self, manifest: &str) -> Result<String> {
        let url = format!("{}/deploy", self.endpoint);
        
        // TODO: Extract capsule_id properly. For now, use a placeholder or extract from manifest if possible.
        // Since we only have the raw string here, we'll generate a temporary ID or rely on the server to parse.
        // Ideally, deploy() should accept capsule_id as an argument.
        // For this step, we'll assume the manifest contains the name, or we send a generic ID.
        let request_body = CloudDeployRequest {
            capsule_id: "unknown-capsule".to_string(), // Should be passed in
            manifest: manifest.to_string(),
        };

        info!("Sending deployment request to {}", url);

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request_body)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to send deployment request: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            error!("Cloud deployment failed: {} - {}", status, error_text);
            return Err(anyhow!("Cloud deployment failed: {} - {}", status, error_text));
        }

        let deploy_response: CloudDeployResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse deployment response: {}", e))?;

        info!("Cloud deployment initiated. Job ID: {}", deploy_response.job_id);
        Ok(deploy_response.job_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CloudConfig;

    #[tokio::test]
    #[ignore] // Ignored by default, run manually with mock server
    async fn test_deploy_integration() {
        let config = CloudConfig {
            enabled: true,
            api_endpoint: Some("http://localhost:8000".to_string()),
            api_key: Some("test-key".to_string()),
        };

        let client = SkyPilotClient::new(&config).unwrap();
        let result = client.deploy("test-manifest-content").await;
        
        match result {
            Ok(job_id) => println!("Deploy success: {}", job_id),
            Err(e) => panic!("Deploy failed: {}", e),
        }
    }
}
