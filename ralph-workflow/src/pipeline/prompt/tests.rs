use super::*;
use crate::workspace::MemoryWorkspace;
use crate::workspace::Workspace;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

fn test_logger() -> Logger {
    Logger::new(Colors::new())
}

#[test]
fn test_truncate_prompt_small_content() {
    let logger = test_logger();
    let content = "This is a small prompt that fits within limits.";
    let result = truncate_prompt_if_needed(content, &logger);
    assert_eq!(result, content);
}

#[test]
fn test_truncate_prompt_large_content_with_marker() {
    let logger = test_logger();
    // Create content larger than MAX_PROMPT_SIZE with a section separator
    let prefix = "Task: Do something\n\n---\n";
    let large_content = "x".repeat(MAX_PROMPT_SIZE + 50000);
    let content = format!("{}{}", prefix, large_content);

    let result = truncate_prompt_if_needed(&content, &logger);

    // Should be truncated
    assert!(result.len() < content.len());
    // Should have truncation marker
    assert!(result.contains("truncated"));
    // Should preserve the prefix
    assert!(result.starts_with("Task:"));
}

#[test]
fn test_truncate_prompt_large_content_fallback() {
    let logger = test_logger();
    // Create content larger than MAX_PROMPT_SIZE without any markers
    let content = "a".repeat(MAX_PROMPT_SIZE + 50000);

    let result = truncate_prompt_if_needed(&content, &logger);

    // Should be truncated
    assert!(result.len() < content.len());
    // Should have truncation marker
    assert!(result.contains("truncated"));
}

#[test]
fn test_truncate_prompt_preserves_end() {
    let logger = test_logger();
    // Content with marker and important end content
    let prefix = "Instructions\n\n---\n";
    let middle = "m".repeat(MAX_PROMPT_SIZE);
    let suffix = "\nIMPORTANT_END_MARKER";
    let content = format!("{}{}{}", prefix, middle, suffix);

    let result = truncate_prompt_if_needed(&content, &logger);

    // Should preserve the end content (most relevant for XSD errors)
    assert!(result.contains("IMPORTANT_END_MARKER"));
}

#[test]
fn test_build_prompt_archive_filename_is_unique_across_calls_with_same_timestamp() {
    let a = build_prompt_archive_filename(
        "planning",
        "codex",
        ".agent/logs/planning_1",
        Some(0),
        Some(0),
        123,
    );
    let b = build_prompt_archive_filename(
        "planning",
        "codex",
        ".agent/logs/planning_1",
        Some(0),
        Some(0),
        123,
    );

    assert_ne!(a, b);
    assert!(a.ends_with("_123.txt"));
    assert!(b.ends_with("_123.txt"));
}

#[test]
fn test_streaming_line_reader_rejects_single_line_larger_than_max_buffer_size() {
    // Regression test: BufRead::lines() must not accumulate unbounded memory
    // when the stream never emits a newline.
    let data = vec![b'a'; MAX_BUFFER_SIZE + 1];
    let reader = StreamingLineReader::new(Cursor::new(data));

    let mut lines = reader.lines();
    let first = lines.next().expect("expected one line or an error");

    assert!(
        first.is_err(),
        "expected an error when a single line exceeds MAX_BUFFER_SIZE"
    );
}

struct CountingReader {
    data: Vec<u8>,
    pos: usize,
    total_read: Arc<AtomicUsize>,
}

impl CountingReader {
    fn new(data: Vec<u8>, total_read: Arc<AtomicUsize>) -> Self {
        Self {
            data,
            pos: 0,
            total_read,
        }
    }
}

impl Read for CountingReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.data.len() {
            return Ok(0);
        }
        let remaining = self.data.len() - self.pos;
        let n = remaining.min(buf.len());
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        self.total_read.fetch_add(n, Ordering::SeqCst);
        Ok(n)
    }
}

fn strip_prompt_archive_sequence(filename: &str) -> String {
    let without_ext = filename
        .strip_suffix(".txt")
        .expect("archive filename should end with .txt");
    let mut parts: Vec<&str> = without_ext.split('_').collect();
    assert!(
        parts.len() >= 3,
        "unexpected archive filename shape: {filename}"
    );

    let timestamp = parts.pop().expect("timestamp");
    let seq = parts.pop().expect("sequence");
    assert!(
        seq.starts_with('s') && seq[1..].chars().all(|c| c.is_ascii_digit()),
        "expected sequence segment like s123, got '{seq}' in '{filename}'"
    );

    parts.push(timestamp);
    format!("{}.txt", parts.join("_"))
}

#[test]
fn test_collect_stderr_with_cap_drains_to_eof() {
    let total_read = Arc::new(AtomicUsize::new(0));
    let data = (0..100u8).collect::<Vec<u8>>();
    let reader = CountingReader::new(data.clone(), Arc::clone(&total_read));

    let result = collect_stderr_with_cap_and_drain(reader, 10).unwrap();
    assert!(result.contains("<stderr truncated>"));
    assert_eq!(total_read.load(Ordering::SeqCst), data.len());
}

#[test]
fn test_build_prompt_archive_filename_from_structured_log_components() {
    let name = build_prompt_archive_filename(
        "planning",
        "ccs/glm",
        ".agent/logs/planning_1",
        Some(0),
        Some(2),
        123,
    );
    assert_eq!(
        strip_prompt_archive_sequence(&name),
        "planning_1_ccs-glm_0_a2_123.txt"
    );
    assert!(!name.contains(".log"));
}

#[test]
fn test_build_prompt_archive_filename_for_review_logs_without_agent_in_name() {
    let name = build_prompt_archive_filename(
        "review",
        "codex",
        ".agent/logs/reviewer_review_2",
        None,
        None,
        42,
    );
    assert_eq!(
        strip_prompt_archive_sequence(&name),
        "reviewer_review_2_codex_42.txt"
    );
}

#[test]
fn test_build_prompt_archive_filename_dedupes_when_logfile_is_agent_only() {
    let name = build_prompt_archive_filename("dev", "claude", ".agent/logs/claude", None, None, 7);
    assert_eq!(strip_prompt_archive_sequence(&name), "dev_claude_7.txt");
}

#[test]
fn test_build_prompt_archive_filename_does_not_depend_on_logfile_stem_parsing() {
    // Agent names may contain underscores. The archive filename should remain stable
    // and should not attempt to reverse-parse delimiters from the logfile stem.
    let name = build_prompt_archive_filename(
        "planning",
        "openai/gpt_4o",
        ".agent/logs/planning_1",
        Some(0),
        Some(2),
        123,
    );
    assert_eq!(
        strip_prompt_archive_sequence(&name),
        "planning_1_openai-gpt_4o_0_a2_123.txt"
    );
}

#[test]
fn test_run_with_agent_spawn_creates_parent_directory_for_logfile() {
    use crate::executor::MockProcessExecutor;
    use std::io;
    use std::path::Path;

    #[derive(Debug)]
    struct StrictLogsWorkspace {
        inner: MemoryWorkspace,
        logs_created: AtomicBool,
    }

    impl StrictLogsWorkspace {
        fn new(inner: MemoryWorkspace) -> Self {
            Self {
                inner,
                logs_created: AtomicBool::new(false),
            }
        }
    }

    impl Workspace for StrictLogsWorkspace {
        fn root(&self) -> &Path {
            self.inner.root()
        }

        fn read(&self, relative: &Path) -> io::Result<String> {
            self.inner.read(relative)
        }

        fn read_bytes(&self, relative: &Path) -> io::Result<Vec<u8>> {
            self.inner.read_bytes(relative)
        }

        fn write(&self, relative: &Path, content: &str) -> io::Result<()> {
            if relative == Path::new(".agent/logs/test.log")
                && !self.logs_created.load(Ordering::Acquire)
            {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "parent dir missing (strict workspace)",
                ));
            }
            self.inner.write(relative, content)
        }

        fn write_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
            self.inner.write_bytes(relative, content)
        }

        fn append_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
            if relative == Path::new(".agent/logs/test.log")
                && !self.logs_created.load(Ordering::Acquire)
            {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "parent dir missing (strict workspace)",
                ));
            }
            self.inner.append_bytes(relative, content)
        }

        fn exists(&self, relative: &Path) -> bool {
            self.inner.exists(relative)
        }

        fn is_file(&self, relative: &Path) -> bool {
            self.inner.is_file(relative)
        }

        fn is_dir(&self, relative: &Path) -> bool {
            self.inner.is_dir(relative)
        }

        fn remove(&self, relative: &Path) -> io::Result<()> {
            self.inner.remove(relative)
        }

        fn remove_if_exists(&self, relative: &Path) -> io::Result<()> {
            self.inner.remove_if_exists(relative)
        }

        fn remove_dir_all(&self, relative: &Path) -> io::Result<()> {
            self.inner.remove_dir_all(relative)
        }

        fn remove_dir_all_if_exists(&self, relative: &Path) -> io::Result<()> {
            self.inner.remove_dir_all_if_exists(relative)
        }

        fn create_dir_all(&self, relative: &Path) -> io::Result<()> {
            if relative == Path::new(".agent/logs") {
                self.logs_created.store(true, Ordering::Release);
            }
            self.inner.create_dir_all(relative)
        }

        fn read_dir(&self, relative: &Path) -> io::Result<Vec<crate::workspace::DirEntry>> {
            self.inner.read_dir(relative)
        }

        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.inner.rename(from, to)
        }

        fn write_atomic(&self, relative: &Path, content: &str) -> io::Result<()> {
            self.inner.write_atomic(relative, content)
        }

        fn set_readonly(&self, relative: &Path) -> io::Result<()> {
            self.inner.set_readonly(relative)
        }

        fn set_writable(&self, relative: &Path) -> io::Result<()> {
            self.inner.set_writable(relative)
        }
    }

    let workspace = StrictLogsWorkspace::new(MemoryWorkspace::new_test());
    let logger = test_logger();
    let colors = Colors::new();
    let config = Config::default();
    let mut timer = Timer::new();

    let executor: Arc<MockProcessExecutor> = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn crate::executor::ProcessExecutor> = executor.clone();

    let env_vars: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let cmd = PromptCommand {
        label: "test",
        display_name: "test",
        cmd_str: "mock-agent",
        prompt: "hello",
        log_prefix: ".agent/logs/test",
        model_index: None,
        attempt: None,
        logfile: ".agent/logs/test.log",
        parser_type: JsonParserType::Generic,
        env_vars: &env_vars,
    };

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor.as_ref(),
        executor_arc,
        workspace: &workspace,
    };

    let result = run_with_agent_spawn(&cmd, &mut runtime, &[]);
    assert!(result.is_ok(), "expected agent run to succeed");

    // Ensure the logfile was created and is readable even when the parent directory
    // did not exist ahead of time.
    let content = workspace
        .read(std::path::Path::new(cmd.logfile))
        .expect("logfile should be created");
    assert!(!content.is_empty(), "logfile should contain agent output");
}
