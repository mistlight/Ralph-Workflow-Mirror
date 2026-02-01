#![feature(rustc_private)]
#![warn(unused_extern_crates)]

extern crate rustc_ast;
extern crate rustc_span;

use rustc_lint::{EarlyContext, EarlyLintPass, LintContext};
use rustc_span::{FileName, SourceFile, Span};
use std::collections::HashSet;
use std::sync::Arc;

dylint_linting::impl_early_lint! {
    /// ### What it does
    ///
    /// Checks for files that exceed recommended line limits:
    /// - 500+ lines: Consider refactoring
    /// - 1000+ lines: Must refactor
    ///
    /// ### Why is this bad?
    ///
    /// Large files are harder to navigate, understand, and maintain.
    /// They often indicate that a module has too many responsibilities
    /// and should be split into smaller, more focused modules.
    ///
    /// See CODE_STYLE.md for design principles.
    ///
    /// ### Example
    ///
    /// A file with 1500 lines of code should be refactored into
    /// multiple smaller modules.
    pub FILE_TOO_LONG,
    Warn,
    "file exceeds recommended line limits",
    FileTooLong::default()
}

/// Threshold for "consider refactoring" warning
const CONSIDER_REFACTOR_LINES: usize = 500;

/// Threshold for "must refactor" warning
const MUST_REFACTOR_LINES: usize = 1000;

/// Custom lint pass that tracks which files we've already warned about
#[derive(Default)]
pub struct FileTooLong {
    warned_files: HashSet<String>,
}

/// Check if the file is a local project file (not stdlib, cargo registry, or external)
fn is_local_project_file(name: &FileName) -> bool {
    match name {
        FileName::Real(real_name) => {
            if let Some(path) = real_name.local_path() {
                let path_str = path.display().to_string();
                // Skip cargo registry files
                if path_str.contains(".cargo/registry") || path_str.contains(".cargo\\registry") {
                    return false;
                }
                // Skip rustup toolchain files
                if path_str.contains(".rustup") {
                    return false;
                }
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Get the display path for a file
fn get_file_display_path(name: &FileName) -> Option<String> {
    match name {
        FileName::Real(real_name) => real_name.local_path().map(|p| p.display().to_string()),
        _ => None,
    }
}

impl FileTooLong {
    fn check_source_file(&mut self, cx: &EarlyContext<'_>, source_file: &Arc<SourceFile>) {
        // Skip non-local files (stdlib, external crates, cargo registry)
        if !is_local_project_file(&source_file.name) {
            return;
        }

        // Get file path for display and deduplication
        let Some(file_path) = get_file_display_path(&source_file.name) else {
            return;
        };

        // Only warn once per file
        if self.warned_files.contains(&file_path) {
            return;
        }

        let total_lines = source_file.count_lines();

        // Check thresholds (check higher threshold first for clearer messaging)
        // Semantics are inclusive: 500+ warns, 1000+ is MUST refactor.
        if total_lines >= MUST_REFACTOR_LINES {
            self.warned_files.insert(file_path.clone());

            let warning_span = Span::with_root_ctxt(source_file.start_pos, source_file.start_pos);

            cx.span_lint(FILE_TOO_LONG, warning_span, |diag| {
                diag.primary_message(format!(
                    "file has {total_lines} lines (>= {MUST_REFACTOR_LINES}) - MUST refactor"
                ));
                diag.help("this file is too large and MUST be split into smaller modules");
                diag.note("see CODE_STYLE.md for design principles on module organization");
            });
        } else if total_lines >= CONSIDER_REFACTOR_LINES {
            self.warned_files.insert(file_path.clone());

            let warning_span = Span::with_root_ctxt(source_file.start_pos, source_file.start_pos);

            cx.span_lint(FILE_TOO_LONG, warning_span, |diag| {
                diag.primary_message(format!(
                    "file has {total_lines} lines (>= {CONSIDER_REFACTOR_LINES}) - consider refactoring"
                ));
                diag.help("consider splitting this file into smaller, more focused modules");
                diag.note("see CODE_STYLE.md for design principles on module organization");
            });
        }
    }
}

impl EarlyLintPass for FileTooLong {
    fn check_crate(&mut self, cx: &EarlyContext<'_>, _krate: &rustc_ast::Crate) {
        let source_map = cx.sess().source_map();

        // Iterate over all source files in the source map
        source_map.files().iter().for_each(|source_file| {
            self.check_source_file(cx, source_file);
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ui() {
        dylint_testing::ui_test(env!("CARGO_PKG_NAME"), "ui");
    }
}
