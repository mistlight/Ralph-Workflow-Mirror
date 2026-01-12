//! Tests for language detection module.

mod extension_mapping;
mod stack_detection;

use std::fs::{self, File};
use std::path::Path;

fn create_test_file(dir: &Path, name: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    File::create(path).unwrap();
}

