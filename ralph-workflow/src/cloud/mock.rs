//! Mock cloud reporter for testing.

use super::{CloudError, CloudReporter, ProgressUpdate};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum MockCloudCall {
    Progress(ProgressUpdate),
    Heartbeat,
    Completion { success: bool, message: String },
}

/// Mock cloud reporter that records all calls for test verification.
#[derive(Clone)]
pub struct MockCloudReporter {
    calls: Arc<Mutex<Vec<MockCloudCall>>>,
    should_fail: Arc<Mutex<bool>>,
}

impl MockCloudReporter {
    pub fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            should_fail: Arc::new(Mutex::new(false)),
        }
    }

    pub fn set_should_fail(&self, fail: bool) {
        *self.should_fail.lock().unwrap() = fail;
    }

    pub fn calls(&self) -> Vec<MockCloudCall> {
        self.calls.lock().unwrap().clone()
    }

    pub fn progress_count(&self) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| matches!(c, MockCloudCall::Progress(_)))
            .count()
    }

    pub fn heartbeat_count(&self) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| matches!(c, MockCloudCall::Heartbeat))
            .count()
    }
}

impl Default for MockCloudReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl CloudReporter for MockCloudReporter {
    fn report_progress(&self, update: &ProgressUpdate) -> Result<(), CloudError> {
        if *self.should_fail.lock().unwrap() {
            return Err(CloudError::NetworkError("Mock failure".to_string()));
        }
        self.calls
            .lock()
            .unwrap()
            .push(MockCloudCall::Progress(update.clone()));
        Ok(())
    }

    fn heartbeat(&self) -> Result<(), CloudError> {
        if *self.should_fail.lock().unwrap() {
            return Err(CloudError::NetworkError("Mock failure".to_string()));
        }
        self.calls.lock().unwrap().push(MockCloudCall::Heartbeat);
        Ok(())
    }

    fn report_completion(&self, success: bool, message: &str) -> Result<(), CloudError> {
        if *self.should_fail.lock().unwrap() {
            return Err(CloudError::NetworkError("Mock failure".to_string()));
        }
        self.calls.lock().unwrap().push(MockCloudCall::Completion {
            success,
            message: message.to_string(),
        });
        Ok(())
    }
}
