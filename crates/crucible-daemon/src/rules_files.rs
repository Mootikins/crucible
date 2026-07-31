//! Project rules files (`AGENTS.md` and friends) loaded into an agent's
//! system prompt.
//!
//! The loader that read [`crucible_core::config::ContextConfig::rules_files`]
//! lived in `crucible-context` and went with that crate when it was deleted in
//! 2026-02-23 for having zero consumers — which was true of the crate and not
//! of this feature. Lives here, next to the factory that composes the prompt.

use std::path::Path;

/// Read the project's rules files into a prompt section.
///
/// Hierarchical, as `docs/Help/Rules Files.md` promises: the walk starts at the
/// repository root (the nearest ancestor holding a `.git`, else the workspace
/// itself) and descends to the workspace, so a rule written closer to where the
/// work happens is read last and therefore wins. Within one directory the
/// configured order decides.
///
/// Best-effort by design — an unreadable or absent rules file must not stop an
/// agent starting, so failures are skipped silently. That is also why the
/// *absence* of every file yields an empty string rather than a header with
/// nothing under it.
pub(crate) fn load_rules_files(workspace: &Path, rules_files: &[String]) -> String {
    if rules_files.is_empty() {
        return String::new();
    }

    // The OUTERMOST ancestor holding a `.git`, so a worktree nested inside a
    // parent repo still starts its walk at the parent's rules.
    let repo_root = workspace
        .ancestors()
        .filter(|dir| dir.join(".git").exists())
        .last()
        .unwrap_or(workspace);

    // `ancestors()` walks up; the prompt wants root-first, so collect and flip.
    let mut dirs: Vec<&Path> = workspace
        .ancestors()
        .take_while(|dir| dir.starts_with(repo_root))
        .collect();
    dirs.reverse();

    let mut sections = String::new();
    let mut seen: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    for dir in dirs {
        for name in rules_files {
            let path = dir.join(name);
            if !seen.insert(path.clone()) {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            if contents.trim().is_empty() {
                continue;
            }
            sections.push_str(&format!("\n## {}\n\n{}\n", path.display(), contents.trim()));
        }
    }

    if sections.is_empty() {
        return String::new();
    }
    format!(
        "\n# Project rules\n\nInstructions for this project, from its rules files. \
         Later files are closer to the working directory and take precedence.\n{sections}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Root-first, workspace-last — the order `docs/Help/Rules Files.md`
    /// promises, and the one that makes "closer files win" true when a model
    /// reads the prompt top to bottom.
    #[test]
    fn rules_files_load_from_repo_root_down_to_the_workspace() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join(".git")).unwrap();
        let nested = repo.path().join("src/module");
        std::fs::create_dir_all(&nested).unwrap();

        std::fs::write(repo.path().join("AGENTS.md"), "ROOT-RULE").unwrap();
        std::fs::write(repo.path().join("src/AGENTS.md"), "MID-RULE").unwrap();
        std::fs::write(nested.join("AGENTS.md"), "LEAF-RULE").unwrap();
        // A second configured name in the same directory follows config order.
        std::fs::write(nested.join(".rules"), "LEAF-DOT-RULE").unwrap();

        let rules = load_rules_files(&nested, &["AGENTS.md".to_string(), ".rules".to_string()]);

        let root = rules.find("ROOT-RULE").expect("root rules loaded");
        let mid = rules.find("MID-RULE").expect("intermediate rules loaded");
        let leaf = rules.find("LEAF-RULE").expect("workspace rules loaded");
        let leaf_dot = rules.find("LEAF-DOT-RULE").expect(".rules loaded");
        assert!(
            root < mid && mid < leaf,
            "closer directories come later: {rules}"
        );
        assert!(
            leaf < leaf_dot,
            "within a directory, configured order holds: {rules}"
        );
    }

    /// An empty `rules_files` list is a user turning the feature off, not a
    /// request for the defaults.
    #[test]
    fn no_rules_configured_means_no_rules_section() {
        let ws = tempfile::tempdir().unwrap();
        std::fs::write(ws.path().join("AGENTS.md"), "IGNORED").unwrap();
        assert_eq!(load_rules_files(ws.path(), &[]), "");
    }

    /// No rules files present must not leave an empty heading in the prompt.
    #[test]
    fn missing_rules_files_add_nothing_to_the_prompt() {
        let ws = tempfile::tempdir().unwrap();
        assert_eq!(load_rules_files(ws.path(), &["AGENTS.md".to_string()]), "");
    }
}
