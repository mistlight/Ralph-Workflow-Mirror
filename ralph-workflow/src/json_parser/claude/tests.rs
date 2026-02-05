// Claude parser tests.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json_parser::printer::{SharedPrinter, TestPrinter};

    #[test]
    fn test_printer_method_accessible() {
        // Test that the printer() method is accessible and returns a SharedPrinter
        let test_printer: SharedPrinter = Rc::new(RefCell::new(TestPrinter::new()));
        let parser =
            ClaudeParser::with_printer(Colors::new(), Verbosity::Normal, Rc::clone(&test_printer));

        // This test verifies the printer() method is accessible
        let _printer_ref = parser.printer();
    }

    #[test]
    fn test_streaming_metrics_method_accessible() {
        // Test that the streaming_metrics() method is accessible
        let test_printer: SharedPrinter = Rc::new(RefCell::new(TestPrinter::new()));
        let parser =
            ClaudeParser::with_printer(Colors::new(), Verbosity::Normal, Rc::clone(&test_printer));

        // This test verifies the streaming_metrics() method is accessible
        let _metrics = parser.streaming_metrics();
    }
}
