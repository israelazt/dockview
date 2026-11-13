//! Guards of the architecture (see `rust/ARCHITECTURE.md`). They fail when
//! someone breaks a rule the design depends on, so the rules do not rot.

use std::fs;
use std::path::{Path, PathBuf};

/// The `rust/` folder of the workspace.
fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Names of the crates a manifest depends on (all `[*dependencies]` tables).
fn dependencies_of(krate: &str) -> Vec<String> {
    let manifest = workspace().join("crates").join(krate).join("Cargo.toml");
    let text = fs::read_to_string(&manifest).unwrap();
    let mut in_dependencies = false;
    let mut names = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            let table = line.trim_matches(|c| c == '[' || c == ']');
            in_dependencies = table == "dependencies" || table.ends_with(".dependencies");
        } else if in_dependencies && !line.starts_with('#') {
            let is_entry = line.contains('=') && !line.starts_with('"') && !line.starts_with(' ');
            if let (true, Some(name)) = (is_entry, line.split(['=', '.', ' ']).next()) {
                if !name.is_empty() && !name.starts_with("Win32_") {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

#[test]
fn each_crate_depends_only_on_what_the_layers_allow() {
    // crate -> every dependency it may have. Dependencies go only downward:
    // domain <- platform, ui, app <- the binary.
    let allowed: [(&str, &[&str]); 5] = [
        ("dockview-domain", &["serde", "serde_json"]),
        (
            "dockview-platform",
            &["dockview-domain", "serde", "serde_json", "windows"],
        ),
        ("dockview-ui", &["dockview-domain", "cosmic-text"]),
        ("dockview-app", &["dockview-domain"]),
        (
            "dockview",
            &[
                "dockview-domain",
                "dockview-platform",
                "dockview-ui",
                "dockview-app",
                "winit",
                "png",
                "windows",
            ],
        ),
    ];
    for (krate, may) in allowed {
        assert!(
            !dependencies_of(krate).is_empty(),
            "no dependencies read for {krate}: the parser is broken"
        );
        for dep in dependencies_of(krate) {
            assert!(
                may.contains(&dep.as_str()),
                "{krate} must not depend on {dep} (allowed: {may:?})"
            );
        }
    }
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Lines with code in the production part of a file (before its inline test
/// module).
fn production_lines(text: &str) -> usize {
    let end = text.find("#[cfg(test)]").unwrap_or(text.len());
    text[..end].lines().filter(|l| !l.trim().is_empty()).count()
}

#[test]
fn no_source_file_is_too_big() {
    const LIMIT: usize = 400;
    const TESTS_LIMIT: usize = 1000;
    let mut files = Vec::new();
    rust_files(&workspace().join("crates"), &mut files);
    let mut too_big = Vec::new();
    for path in files {
        let relative = path.strip_prefix(workspace()).unwrap().to_path_buf();
        // Examples and integration tests are not product code.
        if relative.components().any(|c| {
            let c = c.as_os_str();
            c == "examples" || c == "tests" && !relative.ends_with("tests.rs")
        }) {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let (count, limit) = if path.file_name().is_some_and(|n| n == "tests.rs") {
            (
                text.lines().filter(|l| !l.trim().is_empty()).count(),
                TESTS_LIMIT,
            )
        } else {
            (production_lines(&text), LIMIT)
        };
        if count > limit {
            too_big.push(format!(
                "{} has {count} lines (limit {limit})",
                relative.display()
            ));
        }
    }
    assert!(
        too_big.is_empty(),
        "split these files:\n{}",
        too_big.join("\n")
    );
}
