fn main() {
    // Set CRUCIBLE_BUILD_SHA from git hash for version mismatch detection.
    // The existing check_version() in client/mod.rs compares this between
    // client and daemon — if they differ, the daemon auto-restarts.
    // Without this, both sides report "dev" and stale daemons persist.
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
    {
        if output.status.success() {
            let sha = String::from_utf8_lossy(&output.stdout);
            let sha = sha.trim();
            if !sha.is_empty() {
                println!("cargo:rustc-env=CRUCIBLE_BUILD_SHA={sha}");
            }
        }
    }
    // If git is unavailable (CI archive, cargo publish), don't set the var.
    // option_env! in the source will return None, falling back to "dev".

    // Only re-run when HEAD changes (new commit), not on every build.
    // Use CARGO_MANIFEST_DIR to find workspace root reliably.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = std::path::Path::new(&manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(std::path::Path::new("."));
    let git_head = workspace_root.join(".git").join("HEAD");
    if git_head.exists() {
        println!("cargo:rerun-if-changed={}", git_head.display());
        // Also watch the ref that HEAD points to (e.g., refs/heads/master)
        if let Ok(head_content) = std::fs::read_to_string(&git_head) {
            if let Some(ref_path) = head_content.trim().strip_prefix("ref: ") {
                let ref_file = workspace_root.join(".git").join(ref_path);
                if ref_file.exists() {
                    println!("cargo:rerun-if-changed={}", ref_file.display());
                }
            }
        }
    }
}
