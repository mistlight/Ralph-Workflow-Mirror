// Signature file collection and detection results.
//
// This part contains constants, utility functions, SignatureFiles struct,
// and the collection function.

/// Maximum number of files to scan
const MAX_FILES_TO_SCAN: usize = 2000;

/// Maximum number of signature files to collect (across all types).
const MAX_SIGNATURE_FILES: usize = 50;

/// Helper to push unique values to a vector.
fn push_unique(vec: &mut Vec<String>, value: impl Into<String>) {
    let value = value.into();
    if !vec.iter().any(|v| v == &value) {
        vec.push(value);
    }
}

/// Combine multiple items into a single string.
fn combine_unique(items: &[String]) -> Option<String> {
    match items.len() {
        0 => None,
        1 => Some(items[0].clone()),
        _ => Some(
            items
                .iter()
                .map(std::string::String::as_str)
                .collect::<Vec<_>>()
                .join(" + "),
        ),
    }
}

/// Container for signature files found during scanning.
#[derive(Default)]
struct SignatureFiles {
    by_name_lower: HashMap<String, Vec<PathBuf>>,
}

/// Collect signature files using workspace.
fn collect_signature_files_with_workspace(
    workspace: &dyn Workspace,
    root: &Path,
) -> SignatureFiles {
    let targets: HashSet<&str> = [
        "cargo.toml",
        "pyproject.toml",
        "requirements.txt",
        "setup.py",
        "pipfile",
        "package.json",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "bun.lockb",
        "gemfile",
        "go.mod",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "composer.json",
        "mix.exs",
        "pubspec.yaml",
    ]
    .into_iter()
    .collect();

    let mut result = SignatureFiles::default();
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));

    let mut scanned_entries: usize = 0;
    let mut collected: usize = 0;

    while let Some((dir, depth)) = queue.pop_front() {
        if scanned_entries >= MAX_FILES_TO_SCAN || collected >= MAX_SIGNATURE_FILES {
            break;
        }

        let Ok(entries) = workspace.read_dir(&dir) else {
            continue;
        };

        for entry in entries {
            if scanned_entries >= MAX_FILES_TO_SCAN || collected >= MAX_SIGNATURE_FILES {
                break;
            }
            scanned_entries += 1;

            let path = entry.path().to_path_buf();
            let Some(name_os) = entry.file_name() else {
                continue;
            };
            let name = name_os.to_string_lossy().to_string();
            let name_lower = name.to_lowercase();

            if entry.is_dir() {
                if should_skip_dir_name(&name_lower) {
                    continue;
                }
                if depth < MAX_SIGNATURE_SEARCH_DEPTH {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }

            if !entry.is_file() {
                continue;
            }

            // Check if this is a target signature file
            if targets.contains(name_lower.as_str()) {
                result
                    .by_name_lower
                    .entry(name_lower)
                    .or_default()
                    .push(path);
                collected += 1;
            }
        }
    }

    result
}

/// Detection results accumulator.
struct DetectionResults {
    frameworks: Vec<String>,
    test_frameworks: Vec<String>,
    package_managers: Vec<String>,
}

impl DetectionResults {
    const fn new() -> Self {
        Self {
            frameworks: Vec::new(),
            test_frameworks: Vec::new(),
            package_managers: Vec::new(),
        }
    }

    fn push_framework(&mut self, framework: impl Into<String>) {
        push_unique(&mut self.frameworks, framework);
    }

    fn push_test_framework(&mut self, framework: impl Into<String>) {
        push_unique(&mut self.test_frameworks, framework);
    }

    fn push_package_manager(&mut self, manager: impl Into<String>) {
        push_unique(&mut self.package_managers, manager);
    }

    fn finish(self) -> (Vec<String>, Option<String>, Option<String>) {
        (
            self.frameworks,
            combine_unique(&self.test_frameworks),
            combine_unique(&self.package_managers),
        )
    }
}
