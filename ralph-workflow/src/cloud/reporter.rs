//! Cloud reporter trait and implementations.

use super::types::{CloudError, PipelineResult, ProgressUpdate};
use crate::config::types::CloudConfig;

/// Reports pipeline progress to a cloud API.
///
/// This trait abstracts cloud API communication to support multiple
/// implementations (production HTTP, mock for testing, noop for CLI).
pub trait CloudReporter: Send + Sync {
    /// Report a progress update to the cloud.
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails.
    fn report_progress(&self, update: &ProgressUpdate) -> Result<(), CloudError>;

    /// Send a heartbeat to indicate the container is alive.
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails.
    fn heartbeat(&self) -> Result<(), CloudError>;

    /// Report pipeline completion with final results.
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails.
    fn report_completion(&self, result: &PipelineResult) -> Result<(), CloudError>;
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

    fn report_completion(&self, _result: &PipelineResult) -> Result<(), CloudError> {
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
    #[must_use]
    pub const fn new(config: CloudConfig) -> Self {
        Self { config }
    }

    fn build_url(api_url: &str, path: &str) -> Result<String, CloudError> {
        let base = api_url.trim();
        if !base.to_ascii_lowercase().starts_with("https://") {
            return Err(CloudError::Configuration(
                "Cloud API URL must use https://".to_string(),
            ));
        }

        let base = base.trim_end_matches('/');
        let path = path.trim_start_matches('/');

        if path.is_empty() {
            return Ok(base.to_string());
        }

        Ok(format!("{base}/{path}"))
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

        let url = Self::build_url(api_url, path)?;

        // Build HTTP agent with timeout
        let agent = ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .timeout_global(Some(std::time::Duration::from_secs(30)))
                // Always return a Response so we can map status + body ourselves.
                .http_status_as_error(false)
                .build(),
        );

        let json_body =
            serde_json::to_value(body).map_err(|e| CloudError::Serialization(e.to_string()))?;

        let response = agent
            .post(&url)
            .header("Authorization", &format!("Bearer {api_token}"))
            .header("Content-Type", "application/json")
            .send_json(json_body);

        match response {
            Ok(mut resp) => {
                let status = resp.status();
                if status.is_success() {
                    Ok(())
                } else {
                    let body = resp.body_mut().read_to_string().unwrap_or_default();
                    Err(CloudError::HttpError(status.as_u16(), body))
                }
            }
            Err(e) => Err(CloudError::NetworkError(e.to_string())),
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

        let path = format!("runs/{run_id}/progress");
        self.post_json(&path, update)
    }

    fn heartbeat(&self) -> Result<(), CloudError> {
        let run_id = self
            .config
            .run_id
            .as_ref()
            .ok_or_else(|| CloudError::Configuration("Run ID not configured".to_string()))?;

        let path = format!("runs/{run_id}/heartbeat");
        let body = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        self.post_json(&path, &body)
    }

    fn report_completion(&self, result: &PipelineResult) -> Result<(), CloudError> {
        let run_id = self
            .config
            .run_id
            .as_ref()
            .ok_or_else(|| CloudError::Configuration("Run ID not configured".to_string()))?;

        let path = format!("runs/{run_id}/complete");
        self.post_json(&path, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::CloudConfig;

    #[test]
    fn test_build_url_trims_slashes_and_joins_paths() {
        let base = "https://api.example.com/v1/";
        let url = HttpCloudReporter::build_url(base, "/runs/run_1/progress").unwrap();
        assert_eq!(
            url, "https://api.example.com/v1/runs/run_1/progress",
            "URL join should avoid double slashes"
        );
    }

    #[test]
    fn test_build_url_rejects_non_https() {
        let err = HttpCloudReporter::build_url("http://api.example.com", "/runs/x").unwrap_err();
        match err {
            CloudError::Configuration(_) => {}
            other => panic!("expected Configuration error, got: {other:?}"),
        }
    }

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

        let result = super::super::types::PipelineResult {
            success: true,
            commit_sha: None,
            pr_url: None,
            push_count: 0,
            last_pushed_commit: None,
            unpushed_commits: Vec::new(),
            last_push_error: None,
            iterations_used: 1,
            review_passes_used: 0,
            issues_found: false,
            duration_secs: 100,
            error_message: None,
        };

        assert!(reporter.report_progress(&update).is_ok());
        assert!(reporter.heartbeat().is_ok());
        assert!(reporter.report_completion(&result).is_ok());
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
