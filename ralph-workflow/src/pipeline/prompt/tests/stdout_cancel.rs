#[test]
#[cfg(unix)]
fn test_run_with_agent_spawn_cancels_stdout_pump_promptly_when_idle_timeout_enforcement_begins() {
    use std::io::{self, Cursor, Read};
    use std::path::Path;
    use std::process::ExitStatus;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    use std::os::unix::process::ExitStatusExt;

    use crate::agents::JsonParserType;
    use crate::config::Config;
    use crate::executor::{AgentChildHandle, AgentSpawnConfig, ProcessExecutor, ProcessOutput};
    use crate::logger::{Colors, Logger};
    use crate::pipeline::Timer;
    use crate::workspace::MemoryWorkspace;

    use super::super::agent_spawn_test::run_with_agent_spawn_with_monitor_config;
    use super::super::types::{PipelineRuntime, PromptCommand};

    #[derive(Debug)]
    struct SharedRunningChild {
        still_running: Arc<AtomicBool>,
    }

    impl crate::executor::AgentChild for SharedRunningChild {
        fn id(&self) -> u32 {
            12345
        }

        fn wait(&mut self) -> io::Result<ExitStatus> {
            while self.still_running.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(ExitStatus::from_raw(0))
        }

        fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            if self.still_running.load(Ordering::Acquire) {
                return Ok(None);
            }
            Ok(Some(ExitStatus::from_raw(0)))
        }
    }

    #[derive(Debug, Clone)]
    struct WouldBlockForever {
        reads: Arc<AtomicUsize>,
    }

    impl Read for WouldBlockForever {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Err(io::Error::from(io::ErrorKind::WouldBlock))
        }
    }

    #[derive(Debug)]
    struct Executor {
        still_running: Arc<AtomicBool>,
        kill_started: Arc<AtomicBool>,
        stdout_reads: Arc<AtomicUsize>,
    }

    impl ProcessExecutor for Executor {
        fn execute(
            &self,
            command: &str,
            _args: &[&str],
            _env: &[(String, String)],
            _workdir: Option<&Path>,
        ) -> io::Result<ProcessOutput> {
            if command == "kill" {
                self.kill_started.store(true, Ordering::Release);
            }
            Ok(ProcessOutput {
                status: ExitStatus::from_raw(0),
                stdout: String::new(),
                stderr: String::new(),
            })
        }

        fn spawn_agent(&self, _config: &AgentSpawnConfig) -> io::Result<AgentChildHandle> {
            let stdout = Box::new(WouldBlockForever {
                reads: Arc::clone(&self.stdout_reads),
            }) as Box<dyn io::Read + Send>;
            let stderr = Box::new(Cursor::new(Vec::<u8>::new())) as Box<dyn io::Read + Send>;
            let inner: Box<dyn crate::executor::AgentChild> = Box::new(SharedRunningChild {
                still_running: Arc::clone(&self.still_running),
            });

            Ok(AgentChildHandle {
                stdout,
                stderr,
                inner,
            })
        }
    }

    let workspace = MemoryWorkspace::new_test();
    let logger = Logger::new(Colors::new());
    let colors = Colors::new();
    let config = Config::test_default();
    let mut timer = Timer::new();

    let still_running = Arc::new(AtomicBool::new(true));
    let kill_started = Arc::new(AtomicBool::new(false));
    let stdout_reads = Arc::new(AtomicUsize::new(0));
    let executor = Arc::new(Executor {
        still_running: Arc::clone(&still_running),
        kill_started: Arc::clone(&kill_started),
        stdout_reads: Arc::clone(&stdout_reads),
    });
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();

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

    std::thread::scope(|scope| {
        let (tx, rx) = mpsc::channel();
        scope.spawn(move || {
            let result = run_with_agent_spawn_with_monitor_config(
                &cmd,
                &mut runtime,
                &[],
                0,
                Duration::from_millis(1),
                crate::pipeline::idle_timeout::KillConfig::new(
                    Duration::from_millis(1),
                    Duration::from_millis(1),
                    Duration::from_millis(1),
                    Duration::from_millis(250),
                    Duration::from_millis(20),
                ),
            );
            let _ = tx.send(result);
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if kill_started.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            kill_started.load(Ordering::Acquire),
            "expected idle-timeout enforcement to begin (kill command executed)"
        );

        // Once enforcement begins, stdout cancellation should stop the stdout pump quickly,
        // even if the monitor continues termination verification for longer.
        let reads_at_start = stdout_reads.load(Ordering::Acquire);
        std::thread::sleep(Duration::from_millis(40));
        let reads_mid = stdout_reads.load(Ordering::Acquire);
        std::thread::sleep(Duration::from_millis(40));
        let reads_end = stdout_reads.load(Ordering::Acquire);

        // With very aggressive timeouts (idle_timeout_secs=0), enforcement may begin before the
        // stdout pump manages to perform its first read attempt. What we care about is that once
        // enforcement begins, stdout reads stop increasing promptly.
        assert_eq!(
            reads_mid, reads_end,
            "expected stdout pump reads to stop shortly after enforcement begins"
        );

        let result = rx
            .recv_timeout(Duration::from_secs(3))
            .expect("expected run to return");
        let result = result.expect("expected successful CommandResult");
        assert_eq!(result.exit_code, 143);

        still_running.store(false, Ordering::Release);
    });
}
