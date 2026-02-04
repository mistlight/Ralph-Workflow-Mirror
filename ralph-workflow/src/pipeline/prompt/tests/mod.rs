use super::*;
use crate::workspace::{MemoryWorkspace, Workspace};
use std::io::{self, Cursor, Read};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

mod archive_filename;
mod spawn_idle_timeout;
mod spawn_logfile;
mod spawn_streaming_error;
mod stderr_collector;
mod streaming_line_reader;
mod truncate;

fn test_logger() -> Logger {
    Logger::new(Colors::new())
}
