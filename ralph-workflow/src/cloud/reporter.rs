//! Cloud reporter trait and implementations.

use super::types::{CloudError, ProgressUpdate};
use crate::config::types::CloudConfig;

/// Reports pipeline progress to a cloud API.
///
/// This trait abstracts cloud API communication to support multiple
/// implementations (production HTTP, mock for testing, noop for CLI).
pub trait CloudReporter: Send + Sync {
    /// Report a progress update to the cloud.
    fn report_progress(&self, update: &ProgressUpdate) -> Result<(), CloudError>;

    /// Send a heartbeat to indicate the container is alive.
    fn heartbeat(&self) -> Result<(), CloudError>;

    /// Report pipeline completion with final results.
    fn report_completion(&self, success: bool, message: &str) -> Result<(), CloudError>;
}

/// No-op cloud reporter for CLI mode.
///
/// This is the default reporter when cloud mode is disabled.
/// All methods are no-ops that return Ok immediately.
pub struct NoopCloudReporter;

impl CloudReporter for NoopCloudReporter {
    fn report_progress(&self, _update: &ProgressUpdate) -> Result<(), CloudError> {
        Ok(())
    }

    fn heartbeat(&self) -> Result<(), CloudError> {
        Ok(())
    }

    fn report_completion(&self, _success: bool, _message: &str) -> Result<(), CloudError> {
        Ok(())
    }
}

/// HTTP cloud reporter for production use.
///
/// Sends progress updates to a cloud API via HTTP POST requests.
pub struct HttpCloudReporter {
    config: CloudConfig,
}

impl HttpCloudReporter {
    pub fn new(config: CloudConfig) -> Self {
        Self { config }
    }

    fn post_json<T: serde::Serialize>(&self, path: &str, body: &T) -> Result<(), CloudError> {
        let api_url = self
            .config
            .api_url
            .as_ref()
            .ok_or_else(|| CloudError::Configuration("API URL not configured".to_string()))?;
        let api_token = self
            .config
            .api_token
            .as_ref()
            .ok_or_else(|| CloudError::Configuration("API token not configured".to_string()))?;

        let url = format!("{}{}", api_url, path);

        // Build HTTP agent with timeout
        let agent = ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .timeout_global(Some(std::time::Duration::from_secs(30)))
                .build(),
        );

        let json_body =
            serde_json::to_value(body).map_err(|e| CloudError::Serialization(e.to_string()))?;

        let response = agent
            .post(&url)
            .header("Authorization", &format!("Bearer {}", api_token))
            .header("Content-Type", "application/json")
            .send_json(json_body);

        match response {
            Ok(_) => Ok(()),
            Err(e) => Err(CloudError::NetworkError(format!("{:?}", e))),
        }
    }
}

impl CloudReporter for HttpCloudReporter {
    fn report_progress(&self, update: &ProgressUpdate) -> Result<(), CloudError> {
        let run_id = self
            .config
            .run_id
            .as_ref()
            .ok_or_else(|| CloudError::Configuration("Run ID not configured".to_string()))?;

        let path = format!("/v1/runs/{}/progress", run_id);
        self.post_json(&path, update)
    }

    fn heartbeat(&self) -> Result<(), CloudError> {
        let run_id = self
            .config
            .run_id
            .as_ref()
            .ok_or_else(|| CloudError::Configuration("Run ID not configured".to_string()))?;

        let path = format!("/v1/runs/{}/heartbeat", run_id);
        let body = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        self.post_json(&path, &body)
    }

    fn report_completion(&self, success: bool, message: &str) -> Result<(), CloudError> {
        let run_id = self
            .config
            .run_id
            .as_ref()
            .ok_or_else(|| CloudError::Configuration("Run ID not configured".to_string()))?;

        let path = format!("/v1/runs/{}/complete", run_id);
        let body = serde_json::json!({
            "success": success,
            "message": message,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        self.post_json(&path, &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::CloudConfig;

    #[test]
    fn test_noop_reporter_returns_ok() {
        let reporter = NoopCloudReporter;
        let update = ProgressUpdate {
            timestamp: "2025-02-15T10:00:00Z".to_string(),
            phase: "Planning".to_string(),
            previous_phase: None,
            iteration: Some(1),
            total_iterations: Some(3),
            review_pass: None,
            total_review_passes: None,
            message: "Test".to_string(),
            event_type: super::super::types::ProgressEventType::PipelineStarted,
        };

        assert!(reporter.report_progress(&update).is_ok());
        assert!(reporter.heartbeat().is_ok());
        assert!(reporter.report_completion(true, "Done").is_ok());
    }

    #[test]
    fn test_http_reporter_requires_config() {
        let config = CloudConfig::disabled();
        let reporter = HttpCloudReporter::new(config);

        let update = ProgressUpdate {
            timestamp: "2025-02-15T10:00:00Z".to_string(),
            phase: "Planning".to_string(),
            previous_phase: None,
            iteration: Some(1),
            total_iterations: Some(3),
            review_pass: None,
            total_review_passes: None,
            message: "Test".to_string(),
            event_type: super::super::types::ProgressEventType::PipelineStarted,
        };

        // Should fail because config is disabled (no URL/token)
        assert!(reporter.report_progress(&update).is_err());
    }
}
