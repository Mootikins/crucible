use anyhow::{anyhow, Result};
use clap::CommandFactory;
use clap_complete::{
    generate,
    shells::{Bash, Fish, Zsh},
};

/// Generate shell completion scripts for bash, zsh, and fish
pub fn execute(shell: &str) -> Result<()> {
    let mut cmd = crate::cli::Cli::command();
    let mut stdout = std::io::stdout();

    match shell {
        "bash" => {
            generate(Bash, &mut cmd, "cru", &mut stdout);
            Ok(())
        }
        "zsh" => {
            generate(Zsh, &mut cmd, "cru", &mut stdout);
            Ok(())
        }
        "fish" => {
            generate(Fish, &mut cmd, "cru", &mut stdout);
            Ok(())
        }
        other => Err(anyhow!(
            "Unsupported shell: {}. Supported: bash, zsh, fish",
            other
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unsupported_shell_returns_error() {
        let result = execute("nushell");
        assert!(result.is_err(), "unsupported shell should return error");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Unsupported shell"),
            "error message should mention unsupported shell"
        );
        assert!(
            err_msg.contains("bash") && err_msg.contains("zsh") && err_msg.contains("fish"),
            "error message should list supported shells"
        );
    }
}
