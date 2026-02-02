// Printer trait and standard implementations.
//
// Contains the Printable trait and StdoutPrinter/StderrPrinter.

/// Trait for output destinations in parsers.
///
/// This trait allows parsers to write to different output destinations
/// (stdout, stderr, or test collectors) without hardcoding the specific
/// destination. This makes parsers testable by allowing output capture.
pub trait Printable: std::io::Write {
    /// Check if this printer is connected to a terminal.
    ///
    /// This is used to determine whether to use terminal-specific features
    /// like colors and carriage return-based updates.
    fn is_terminal(&self) -> bool;
}

/// Printer that writes to stdout.
#[derive(Debug)]
pub struct StdoutPrinter {
    stdout: Stdout,
    is_terminal: bool,
}

impl StdoutPrinter {
    /// Create a new stdout printer.
    pub fn new() -> Self {
        let is_terminal = std::io::stdout().is_terminal();
        Self {
            stdout: std::io::stdout(),
            is_terminal,
        }
    }
}

impl Default for StdoutPrinter {
    fn default() -> Self {
        Self::new()
    }
}

impl std::io::Write for StdoutPrinter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stdout.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stdout.flush()
    }
}

impl Printable for StdoutPrinter {
    fn is_terminal(&self) -> bool {
        self.is_terminal
    }
}

/// Printer that writes to stderr.
#[derive(Debug)]
#[cfg(any(test, feature = "test-utils"))]
pub struct StderrPrinter {
    stderr: Stderr,
    is_terminal: bool,
}

#[cfg(any(test, feature = "test-utils"))]
impl StderrPrinter {
    /// Create a new stderr printer.
    pub fn new() -> Self {
        let is_terminal = std::io::stderr().is_terminal();
        Self {
            stderr: std::io::stderr(),
            is_terminal,
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Default for StderrPrinter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl std::io::Write for StderrPrinter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stderr.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stderr.flush()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Printable for StderrPrinter {
    fn is_terminal(&self) -> bool {
        self.is_terminal
    }
}

/// Shared printer reference for use in parsers.
///
/// This type alias represents a shared, mutable reference to a printer
/// that can be used across parser methods.
pub type SharedPrinter = Rc<RefCell<dyn Printable>>;

/// Create a shared stdout printer.
pub fn shared_stdout() -> SharedPrinter {
    Rc::new(RefCell::new(StdoutPrinter::new()))
}
