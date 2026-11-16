//! Guards of the architecture (see `ARCHITECTURE.md`). They fail when
//! someone breaks a rule the design depends on, so the rules do not rot.

use std::fs;
use std::path::{Path, PathBuf};

/// The root folder of the workspace.
fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Where a crate lives: the libraries under `crates/`, the programs under
/// `apps/` (the program is still called `dockview`; its folder is `luminifex`).
fn crate_dir(krate: &str) -> PathBuf {
    if krate == "dockview" {
        return workspace().join("apps").join("luminifex");
    }
    let app = workspace().join("apps").join(krate);
    if app.is_dir() {
        app
    } else {
        workspace().join("crates").join(krate)
    }
}

/// Every folder with Rust sources of the workspace.
fn source_roots() -> Vec<PathBuf> {
    ["crates", "apps"]
        .iter()
        .map(|d| workspace().join(d))
        .filter(|d| d.is_dir())
        .collect()
}

/// Names of the crates a manifest depends on (all `[*dependencies]` tables).
fn dependencies_of(krate: &str) -> Vec<String> {
    let manifest = crate_dir(krate).join("Cargo.toml");
    let text = fs::read_to_string(&manifest).unwrap();
    let mut in_dependencies = false;
    let mut names = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            let table = line.trim_matches(|c| c == '[' || c == ']');
            in_dependencies = table == "dependencies"
                || table == "build-dependencies"
                || table.ends_with(".dependencies");
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
    let allowed: [(&str, &[&str]); 6] = [
        ("dockview-domain", &["serde", "serde_json"]),
        (
            "dockview-stream",
            &["dockview-domain", "rustls", "webpki-roots"],
        ),
        (
            "dockview-platform",
            &[
                "dockview-domain",
                "dockview-stream",
                "serde",
                "serde_json",
                "windows",
            ],
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
                // Build-time only: the icon and the manifest (see build.rs).
                "winresource",
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

/// Lines with code in the production part of a file (before its
/// `#[cfg(test)] mod tests`).
fn production_lines(text: &str) -> usize {
    before_the_tests_module(text)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count()
}

#[test]
fn no_source_file_is_too_big() {
    const LIMIT: usize = 400;
    const TESTS_LIMIT: usize = 1000;
    let mut files = Vec::new();
    for root in source_roots() {
        rust_files(&root, &mut files);
    }
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

/// The part of a file before its `#[cfg(test)] mod tests`. Other items marked
/// `#[cfg(test)]` (a helper module declared in `lib.rs`, say) do not end the
/// production code.
fn before_the_tests_module(text: &str) -> &str {
    let mut from = 0;
    while let Some(found) = text[from..].find("#[cfg(test)]") {
        let at = from + found;
        let after = text[at + "#[cfg(test)]".len()..].trim_start();
        if after.starts_with("mod tests") {
            return &text[..at];
        }
        from = at + 1;
    }
    text
}

/// Where the program may still print straight to the console, and why. Every
/// other message goes through the log (`log_info!`, `log_warn!`, `log_error!`),
/// which reaches the file and, when there is one, the console.
const PRINT_EXCEPTIONS: [(&str, &str); 4] = [
    (
        "luminifex/src/shell/window.rs",
        "`--screenshot` reports the file it saved on the standard output, for scripts",
    ),
    (
        "dockview-platform/src/log_file.rs",
        "the log's own mirror to the console",
    ),
    (
        "luminifex/build.rs",
        "a build script talks to cargo by printing `cargo:` lines",
    ),
    (
        "dockview-platform/src/test_support.rs",
        "a helper compiled only for tests",
    ),
];

#[test]
fn production_code_logs_instead_of_printing() {
    let mut files = Vec::new();
    for root in source_roots() {
        rust_files(&root, &mut files);
    }
    let mut offenders = Vec::new();
    for path in files {
        let relative = path.strip_prefix(workspace()).unwrap().to_path_buf();
        let shown = relative.to_string_lossy().replace('\\', "/");
        // Examples and tests may print; so may the files listed above.
        if relative
            .components()
            .any(|c| c.as_os_str() == "examples" || c.as_os_str() == "tests")
            || shown.ends_with("/tests.rs")
            || PRINT_EXCEPTIONS
                .iter()
                .any(|(allowed, _)| shown.ends_with(allowed))
        {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let production = before_the_tests_module(&text);
        for (number, line) in production.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if ["println!(", "eprintln!(", "print!(", "eprint!("]
                .iter()
                .any(|p| code.contains(p))
            {
                offenders.push(format!("{shown}:{}", number + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "use log_info!/log_warn!/log_error! instead of printing (or add an exception with its reason):\n{}",
        offenders.join("\n")
    );
}

#[test]
fn only_the_tests_module_ends_the_production_code() {
    let file = "fn a() {}
#[cfg(test)]
mod testing;
fn b() {}
#[cfg(test)]
mod tests {
fn c() {}
}
";
    let production = before_the_tests_module(file);
    assert!(production.contains("fn b()"));
    assert!(!production.contains("fn c()"));
    assert_eq!(before_the_tests_module("fn only() {}"), "fn only() {}");
}

// -- The modes do not know each other ------------------------------------------

/// The modules of `dockview-ui` that belong to each mode (see `docs/MODES_PLAN.md`):
/// the viewer, the Studio and Settings. What they share (the engine, the
/// widgets, the texts, the options) is in none of these lists.
const MODE_MODULES: [(&str, &[&str]); 3] = [
    ("the viewer", &["controls", "hud", "hud_cards", "rec"]),
    (
        "the Studio",
        &[
            "studio",
            "studio_status",
            "canvas_bar",
            "scene_editor",
            "rail",
        ],
    ),
    ("Settings", &["settings"]),
];

/// The module a source file of `dockview-ui/src` belongs to (its first path
/// component without the extension).
fn ui_module_of(relative: &Path) -> Option<String> {
    let first = relative.components().next()?.as_os_str().to_string_lossy();
    Some(first.trim_end_matches(".rs").to_string())
}

/// The modules a piece of code reaches with `crate::name` or `super::name`
/// at the top of the crate (also inside braces: `use crate::{a, b}`).
fn crate_paths_in(code: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (at, _) in code.match_indices("crate::") {
        let rest = &code[at + "crate::".len()..];
        if let Some(group) = rest.strip_prefix('{') {
            let inside = group.split('}').next().unwrap_or("");
            for part in inside.split(',') {
                let name: String = part
                    .trim()
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    found.push(name);
                }
            }
        } else {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                found.push(name);
            }
        }
    }
    found
}

#[test]
fn a_mode_of_the_interface_never_reaches_into_another() {
    let src = workspace().join("crates").join("dockview-ui").join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let mut offenders = Vec::new();
    for path in files {
        let relative = path.strip_prefix(&src).unwrap().to_path_buf();
        // Tests may build a whole model with the helpers of another mode.
        let shown = relative.to_string_lossy().replace('\\', "/");
        if shown.contains("tests") {
            continue;
        }
        let Some(module) = ui_module_of(&relative) else {
            continue;
        };
        let Some((mine, _)) = MODE_MODULES
            .iter()
            .find(|(_, names)| names.contains(&module.as_str()))
        else {
            continue;
        };
        let text = fs::read_to_string(&path).unwrap();
        let production = before_the_tests_module(&text);
        let code: String = production
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        for reached in crate_paths_in(&code) {
            for (other, names) in MODE_MODULES {
                if other != *mine && names.contains(&reached.as_str()) {
                    offenders.push(format!("{shown} ({mine}) reaches {reached} ({other})"));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a mode must not use another one's code; share it through the engine instead:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_mode_guard_reads_paths_inside_braces_too() {
    assert_eq!(
        crate_paths_in("use crate::{fit, hud::Hud, studio};"),
        ["fit", "hud", "studio"]
    );
    assert_eq!(crate_paths_in("use crate::input::Pointer;"), ["input"]);
    assert!(crate_paths_in("let x = 1;").is_empty());
}
