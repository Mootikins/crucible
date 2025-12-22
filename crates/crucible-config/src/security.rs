//! Security configuration for shell command execution
//!
//! Provides whitelist/blacklist policies for safe shell command execution
//! with prefix matching for both exact and partial command patterns.

use serde::{Deserialize, Serialize};

/// Policy for shell command execution security
///
/// Implements prefix-based matching with blacklist-first evaluation.
/// Commands are matched by building the full command string from cmd + args.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ShellPolicy {
    /// Commands that are explicitly allowed (prefix matching)
    pub whitelist: Vec<String>,
    /// Commands that are explicitly blocked (prefix matching, takes precedence)
    pub blacklist: Vec<String>,
}

impl Default for ShellPolicy {
    fn default() -> Self {
        Self {
            whitelist: Vec::new(),
            blacklist: Vec::new(),
        }
    }
}

impl ShellPolicy {
    /// Check if command is allowed by policy
    ///
    /// Evaluation order:
    /// 1. Build full command from cmd + args
    /// 2. Check blacklist (deny takes precedence)
    /// 3. Check whitelist
    /// 4. Default deny if not in whitelist
    ///
    /// # Examples
    ///
    /// ```
    /// use crucible_config::ShellPolicy;
    ///
    /// let mut policy = ShellPolicy::default();
    /// policy.whitelist.push("git".to_string());
    /// policy.blacklist.push("rm -rf".to_string());
    ///
    /// assert!(policy.is_allowed("git", &["status"]));
    /// assert!(!policy.is_allowed("rm", &["-rf", "/"]));
    /// ```
    pub fn is_allowed(&self, cmd: &str, args: &[&str]) -> bool {
        // Build full command string
        let full_command = if args.is_empty() {
            cmd.to_string()
        } else {
            format!("{} {}", cmd, args.join(" "))
        };

        // Check blacklist first - deny takes precedence
        for blocked in &self.blacklist {
            if full_command.starts_with(blocked) {
                return false;
            }
        }

        // Check whitelist
        for allowed in &self.whitelist {
            if full_command.starts_with(allowed) {
                return true;
            }
        }

        // Default deny if not in whitelist
        false
    }

    /// Returns default safe commands for typical development workflows
    ///
    /// Includes common read-only and safe development commands:
    /// - Version control: git
    /// - Build tools: cargo, just, make
    /// - Package managers: npm, pnpm, yarn, bun, pip, uv
    /// - Container tools: docker, kubectl, helm
    /// - Unix utilities: cat, ls, find, grep, head, tail, wc, echo, date, pwd, env, which
    pub fn default_whitelist() -> Vec<String> {
        vec![
            // Version control
            "git".to_string(),
            // Build tools
            "cargo".to_string(),
            "just".to_string(),
            "make".to_string(),
            // JavaScript/Node package managers
            "npm".to_string(),
            "npx".to_string(),
            "pnpm".to_string(),
            "yarn".to_string(),
            "bun".to_string(),
            // Python package managers
            "pip".to_string(),
            "uv".to_string(),
            "python".to_string(),
            "python3".to_string(),
            // Container/orchestration tools
            "docker".to_string(),
            "docker-compose".to_string(),
            "kubectl".to_string(),
            "helm".to_string(),
            // Safe Unix utilities
            "cat".to_string(),
            "ls".to_string(),
            "find".to_string(),
            "grep".to_string(),
            "head".to_string(),
            "tail".to_string(),
            "wc".to_string(),
            "echo".to_string(),
            "date".to_string(),
            "pwd".to_string(),
            "env".to_string(),
            "which".to_string(),
            "sh".to_string(),
        ]
    }

    /// Returns always-blocked dangerous commands
    ///
    /// Includes commands that could cause system damage or security issues:
    /// - Privilege escalation: sudo, su, doas
    /// - Destructive operations: rm -rf /, chmod 777, mkfs
    /// - Direct device access: > /dev/sd, dd if=
    pub fn default_blacklist() -> Vec<String> {
        vec![
            // Privilege escalation
            "sudo".to_string(),
            "su".to_string(),
            "doas".to_string(),
            // Destructive filesystem operations
            "rm -rf /".to_string(),
            "rm -rf ~".to_string(),
            "chmod 777".to_string(),
            // Direct device access
            "> /dev/sd".to_string(),
            "mkfs".to_string(),
            "dd if=".to_string(),
        ]
    }

    /// Create policy with default whitelist and blacklist
    ///
    /// # Examples
    ///
    /// ```
    /// use crucible_config::ShellPolicy;
    ///
    /// let policy = ShellPolicy::with_defaults();
    /// assert!(policy.is_allowed("git", &["status"]));
    /// assert!(!policy.is_allowed("sudo", &["rm", "-rf", "/"]));
    /// ```
    pub fn with_defaults() -> Self {
        Self {
            whitelist: Self::default_whitelist(),
            blacklist: Self::default_blacklist(),
        }
    }

    /// Merge overlay policy into this policy
    ///
    /// Extends both whitelist and blacklist, removing duplicates.
    /// Used for configuration layering (base config + user overrides).
    ///
    /// # Examples
    ///
    /// ```
    /// use crucible_config::ShellPolicy;
    ///
    /// let base = ShellPolicy::with_defaults();
    /// let mut overlay = ShellPolicy::default();
    /// overlay.whitelist.push("custom-tool".to_string());
    ///
    /// let merged = base.merge(&overlay);
    /// assert!(merged.is_allowed("git", &["status"])); // from base
    /// assert!(merged.is_allowed("custom-tool", &[])); // from overlay
    /// ```
    pub fn merge(&self, overlay: &ShellPolicy) -> ShellPolicy {
        let mut whitelist = self.whitelist.clone();
        whitelist.extend(overlay.whitelist.iter().cloned());
        whitelist.sort();
        whitelist.dedup();

        let mut blacklist = self.blacklist.clone();
        blacklist.extend(overlay.blacklist.iter().cloned());
        blacklist.sort();
        blacklist.dedup();

        ShellPolicy {
            whitelist,
            blacklist,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_policy_default_is_deny() {
        let policy = ShellPolicy::default();
        assert!(!policy.is_allowed("ls", &[]));
        assert!(!policy.is_allowed("git", &["status"]));
        assert!(!policy.is_allowed("anything", &["at", "all"]));
    }

    #[test]
    fn whitelist_allows_exact_command() {
        let mut policy = ShellPolicy::default();
        policy.whitelist.push("git".to_string());

        // Exact command
        assert!(policy.is_allowed("git", &[]));
        // With args
        assert!(policy.is_allowed("git", &["status"]));
        assert!(policy.is_allowed("git", &["commit", "-m", "test"]));
        // Different command should be denied
        assert!(!policy.is_allowed("cargo", &["build"]));
    }

    #[test]
    fn whitelist_allows_prefix_match() {
        let mut policy = ShellPolicy::default();
        policy.whitelist.push("docker".to_string());
        policy.whitelist.push("docker compose".to_string());

        // "docker compose" allows compose subcommands
        assert!(policy.is_allowed("docker", &["compose", "up"]));
        assert!(policy.is_allowed("docker", &["compose", "down"]));

        // But "docker" alone also works due to prefix
        assert!(policy.is_allowed("docker", &["run", "nginx"]));
        assert!(policy.is_allowed("docker", &["ps"]));
    }

    #[test]
    fn blacklist_takes_precedence() {
        let mut policy = ShellPolicy::default();
        policy.whitelist.push("rm".to_string());
        policy.blacklist.push("rm -rf /".to_string());

        // rm is whitelisted
        assert!(policy.is_allowed("rm", &["file.txt"]));
        // But rm -rf / is blacklisted
        assert!(!policy.is_allowed("rm", &["-rf", "/"]));
        assert!(!policy.is_allowed("rm", &["-rf", "/home"]));
    }

    #[test]
    fn default_whitelist_includes_safe_commands() {
        let whitelist = ShellPolicy::default_whitelist();

        // Version control
        assert!(whitelist.contains(&"git".to_string()));

        // Build tools
        assert!(whitelist.contains(&"cargo".to_string()));
        assert!(whitelist.contains(&"just".to_string()));
        assert!(whitelist.contains(&"make".to_string()));

        // Package managers
        assert!(whitelist.contains(&"npm".to_string()));
        assert!(whitelist.contains(&"pip".to_string()));
        assert!(whitelist.contains(&"yarn".to_string()));

        // Container tools
        assert!(whitelist.contains(&"docker".to_string()));
        assert!(whitelist.contains(&"kubectl".to_string()));

        // Unix utilities
        assert!(whitelist.contains(&"cat".to_string()));
        assert!(whitelist.contains(&"ls".to_string()));
        assert!(whitelist.contains(&"grep".to_string()));
    }

    #[test]
    fn default_blacklist_includes_dangerous_commands() {
        let blacklist = ShellPolicy::default_blacklist();

        // Privilege escalation
        assert!(blacklist.contains(&"sudo".to_string()));
        assert!(blacklist.contains(&"su".to_string()));

        // Destructive operations
        assert!(blacklist.contains(&"rm -rf /".to_string()));
        assert!(blacklist.contains(&"chmod 777".to_string()));
        assert!(blacklist.contains(&"mkfs".to_string()));
    }

    #[test]
    fn with_defaults_creates_working_policy() {
        let policy = ShellPolicy::with_defaults();

        // Safe commands allowed
        assert!(policy.is_allowed("git", &["status"]));
        assert!(policy.is_allowed("cargo", &["build"]));
        assert!(policy.is_allowed("ls", &["-la"]));

        // Dangerous commands blocked
        assert!(!policy.is_allowed("sudo", &["rm", "-rf", "/"]));
        assert!(!policy.is_allowed("rm", &["-rf", "/"]));
        assert!(!policy.is_allowed("chmod", &["777", "/etc"]));
    }

    #[test]
    fn shell_policy_merge_extends_whitelist() {
        let mut base = ShellPolicy::default();
        base.whitelist.push("git".to_string());
        base.whitelist.push("cargo".to_string());
        base.blacklist.push("sudo".to_string());

        let mut overlay = ShellPolicy::default();
        overlay.whitelist.push("custom-tool".to_string());
        overlay.whitelist.push("git".to_string()); // duplicate
        overlay.blacklist.push("dangerous".to_string());

        let merged = base.merge(&overlay);

        // Whitelist extended and deduplicated
        assert_eq!(merged.whitelist.len(), 3);
        assert!(merged.whitelist.contains(&"git".to_string()));
        assert!(merged.whitelist.contains(&"cargo".to_string()));
        assert!(merged.whitelist.contains(&"custom-tool".to_string()));

        // Blacklist extended and deduplicated
        assert_eq!(merged.blacklist.len(), 2);
        assert!(merged.blacklist.contains(&"sudo".to_string()));
        assert!(merged.blacklist.contains(&"dangerous".to_string()));
    }

    #[test]
    fn blacklist_prefix_prevents_variations() {
        let mut policy = ShellPolicy::default();
        policy.whitelist.push("rm".to_string());
        policy.blacklist.push("rm -rf".to_string());

        // rm -rf variants are blocked
        assert!(!policy.is_allowed("rm", &["-rf", "/tmp"]));
        assert!(!policy.is_allowed("rm", &["-rf", "~"]));
        assert!(!policy.is_allowed("rm", &["-rfv", "/"]));

        // Other rm commands allowed
        assert!(policy.is_allowed("rm", &["file.txt"]));
        assert!(policy.is_allowed("rm", &["-i", "test.txt"]));
    }
}
