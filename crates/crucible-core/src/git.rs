//! A `git` process that works on the directory its caller names.

/// The variables that select a repository for git. The list is the output of
/// `git rev-parse --local-env-vars`. Git clears the same list when it starts
/// git in a submodule.
///
/// A process that runs inside `git rebase --exec` or a git hook inherits
/// some of these. Git obeys them over the current directory and over `-C`.
/// Thus an inherited `GIT_DIR` sends every command to the repository of the
/// parent process.
pub const REPOSITORY_ENV_VARS: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// A `git` command without the [`REPOSITORY_ENV_VARS`] of this process.
///
/// Use it for all git work on a directory that the caller names. Git then
/// finds the repository from `current_dir` or `-C`. For a tokio process, use
/// `tokio::process::Command::from(command())`.
#[must_use]
pub fn command() -> std::process::Command {
    let mut cmd = std::process::Command::new("git");
    for var in REPOSITORY_ENV_VARS {
        cmd.env_remove(var);
    }
    cmd
}
