//! Guards for `AdwActionRow::activatable_widget` use.
//!
//! Pointing `activatable_widget` at a widget that cannot be activated **and**
//! lives inside that same row creates an unbounded loop:
//!
//! ```text
//! row activate -> mnemonic_activate on child -> propagates back to row -> ...
//! ```
//!
//! GTK does not reject this, so it only shows up as a stack overflow at runtime.
//! That is exactly how the settings page crashed once a `Label` was used as the
//! activatable widget of its own row, so the pattern is checked here instead of
//! being rediscovered by a crash.
//!
//! The whole module is test-only: it holds no runtime behaviour, so compiling it
//! into a release binary would be dead weight.

#![cfg(test)]

/// Widget types that legitimately handle activation.
///
/// Anything outside this list either cannot be activated at all (`Label`,
/// `Image`, `ProgressBar`) or has no activation semantics of its own, which is
/// what turns the row's activation into a cycle.
const ACTIVATABLE_TYPES: &[&str] = &[
    "Entry",
    "SearchEntry",
    "Switch",
    "Button",
    "ToggleButton",
    "LinkButton",
    "CheckButton",
    "MenuButton",
    "SpinButton",
    "ComboBoxText",
    "DropDown",
];

/// Extracts `let name = Type::builder(...)` / `let name = Type::new(...)` pairs.
fn declared_types(source: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();

    // Deliberately simple parsing: the UI code declares widgets in a uniform
    // `let x = gtk4::Type::builder()` shape, and a full parser would be
    // disproportionate for a guard.
    for line in source.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("let ") else {
            continue;
        };

        let Some((name, rhs)) = rest.split_once('=') else {
            continue;
        };
        let name = name.trim().trim_end_matches(':').trim();
        // Skip type annotations such as `let x: Foo = ...`.
        let name = name.split(':').next().unwrap_or(name).trim();

        let rhs = rhs.trim();
        // Strip a turbofish/namespace: `gtk4::Entry::builder()` -> `Entry`.
        let rhs = rhs.split("::").collect::<Vec<_>>();
        if rhs.len() < 2 {
            continue;
        }
        let type_name = rhs[rhs.len() - 2].trim();
        let constructor = rhs[rhs.len() - 1];

        if constructor.starts_with("builder") || constructor.starts_with("new") {
            result.push((name.to_string(), type_name.to_string()));
        }
    }

    result
}

/// Extracts the variable names passed to `activatable_widget(&name)`.
fn activatable_arguments(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = source;

    while let Some(idx) = rest.find("activatable_widget(&") {
        let after = &rest[idx + "activatable_widget(&".len()..];
        if let Some(end) = after.find(')') {
            let name = after[..end].trim().to_string();
            if !name.is_empty() {
                names.push(name);
            }
            rest = &after[end..];
        } else {
            break;
        }
    }

    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ui_sources() -> Vec<(String, String)> {
        // The UI modules are included at compile time so this test cannot be
        // fooled by a missing file at runtime.
        [
            ("dashboard", include_str!("dashboard.rs")),
            ("routing", include_str!("routing.rs")),
            ("servers", include_str!("servers.rs")),
            ("settings", include_str!("settings.rs")),
            ("subscriptions", include_str!("subscriptions.rs")),
            ("window", include_str!("window.rs")),
        ]
        .iter()
        .map(|(name, src)| (name.to_string(), src.to_string()))
        .collect()
    }

    /// The regression that caused the stack overflow: settings.rs pointed an
    /// ActionRow's `activatable_widget` at a Label.
    #[test]
    fn no_action_row_targets_a_non_activatable_widget() {
        let mut problems = Vec::new();

        for (module, source) in ui_sources() {
            let types = declared_types(&source);

            for target in activatable_arguments(&source) {
                let Some((_, type_name)) = types.iter().find(|(name, _)| name == &target) else {
                    // Declared elsewhere (for example a field on the view
                    // struct); those are not the risky inline pattern.
                    continue;
                };

                if !ACTIVATABLE_TYPES.contains(&type_name.as_str()) {
                    problems.push(format!(
                        "{module}.rs: activatable_widget points at '{target}' of type \
                         '{type_name}', which cannot be activated; this recurses until the \
                         stack overflows"
                    ));
                }
            }
        }

        assert!(
            problems.is_empty(),
            "unsafe activatable_widget usage:\n  {}",
            problems.join("\n  ")
        );
    }

    /// The parser driving the check must actually find widgets, otherwise the
    /// guard would pass vacuously and hide real problems.
    #[test]
    fn type_extraction_finds_real_widgets() {
        let source = r#"
            let socks_port_entry = Entry::builder().text("10808").build();
            let auto_proxy_switch = Switch::builder().active(true).build();
            let name = users.get(0);
        "#;

        let types = declared_types(source);
        assert!(
            types.contains(&("socks_port_entry".to_string(), "Entry".to_string())),
            "got: {types:?}"
        );
        assert!(types.contains(&("auto_proxy_switch".to_string(), "Switch".to_string())));
        assert!(!types.iter().any(|(n, _)| n == "name"));
    }

    #[test]
    fn argument_extraction_handles_multiple_calls() {
        let source = r#"
            .activatable_widget(&a_switch)
            .activatable_widget(&an_entry)
        "#;
        assert_eq!(
            activatable_arguments(source),
            vec!["a_switch".to_string(), "an_entry".to_string()]
        );
    }

    /// Documents why the guard exists, and pins the rule it enforces.
    #[test]
    fn non_activatable_types_are_rejected_by_the_allowlist() {
        for bad in ["Label", "Image", "ProgressBar", "Separator"] {
            assert!(
                !ACTIVATABLE_TYPES.contains(&bad),
                "{bad} must not be considered activatable"
            );
        }
        for good in ["Entry", "Switch", "Button"] {
            assert!(ACTIVATABLE_TYPES.contains(&good));
        }
    }

    /// The real UI sources must contain activatable rows at all; an empty scan
    /// would make the main guard meaningless.
    #[test]
    fn ui_sources_actually_use_activatable_widget() {
        let total: usize = ui_sources()
            .iter()
            .map(|(_, src)| activatable_arguments(src).len())
            .sum();
        assert!(
            total > 0,
            "no activatable_widget usages found; the guard is scanning nothing"
        );
    }
}
