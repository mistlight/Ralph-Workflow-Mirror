//! Common types for signature detection results

use super::core::{combine_unique, push_unique};

/// Detection results accumulator
pub struct DetectionResults {
    pub frameworks: Vec<String>,
    pub test_frameworks: Vec<String>,
    pub package_managers: Vec<String>,
}

impl DetectionResults {
    pub fn new() -> Self {
        Self {
            frameworks: Vec::new(),
            test_frameworks: Vec::new(),
            package_managers: Vec::new(),
        }
    }

    pub fn push_framework(&mut self, framework: impl Into<String>) {
        push_unique(&mut self.frameworks, framework);
    }

    pub fn push_test_framework(&mut self, framework: impl Into<String>) {
        push_unique(&mut self.test_frameworks, framework);
    }

    pub fn push_package_manager(&mut self, manager: impl Into<String>) {
        push_unique(&mut self.package_managers, manager);
    }

    pub fn finish(self) -> (Vec<String>, Option<String>, Option<String>) {
        (
            self.frameworks,
            combine_unique(&self.test_frameworks),
            combine_unique(&self.package_managers),
        )
    }
}
