//! Diagnostic tests for ONNX Runtime on Windows
//!
//! These tests help diagnose ONNX Runtime compatibility issues on Windows,
//! particularly C runtime library mismatches and DLL loading problems.
//!
//! Run with:
//! ```bash
//! cargo test -p crucible-llm --features fastembed --test test_onnx_windows_diagnostics -- --nocapture
//! ```

#[cfg(feature = "fastembed")]
mod fastembed_diagnostics {
    use crucible_llm::embeddings::{create_provider, EmbeddingConfig};
    use std::env;

    /// Test that verifies ONNX Runtime can be initialized on Windows
    #[tokio::test]
    #[cfg(target_os = "windows")]
    async fn test_onnx_runtime_initialization_windows() {
        println!("=== ONNX Runtime Windows Diagnostic Test ===");
        println!();

        // Log environment information
        println!("Environment Information:");
        println!("  OS: Windows");
        println!("  Target: {}", env::var("TARGET").unwrap_or_else(|_| "unknown".to_string()));
        if let Ok(rustc) = std::process::Command::new("rustc").arg("--version").output() {
            if let Ok(version) = String::from_utf8(rustc.stdout) {
                println!("  Rust Version: {}", version.trim());
            }
        }
        println!();

        // Check for Visual C++ Redistributable
        println!("Checking for Visual C++ Redistributable...");
        let vc_redist_installed = std::path::Path::new("C:\\Windows\\System32\\msvcp140.dll").exists()
            || std::path::Path::new("C:\\Windows\\System32\\vcruntime140.dll").exists();
        println!("  VCRuntime DLLs found: {}", vc_redist_installed);
        if !vc_redist_installed {
            println!("  WARNING: Visual C++ Redistributable may not be installed");
            println!("  Download from: https://aka.ms/vs/17/release/vc_redist.x64.exe");
        }
        println!();

        // Check environment variables
        println!("Environment Variables:");
        if let Ok(ort_threads) = env::var("ORT_NUM_THREADS") {
            println!("  ORT_NUM_THREADS: {}", ort_threads);
        } else {
            println!("  ORT_NUM_THREADS: not set (using default)");
        }
        println!();

        // Try to create FastEmbed provider
        println!("Attempting to create FastEmbed provider...");
        let config = EmbeddingConfig::fastembed(None, None, None);
        
        match create_provider(config).await {
            Ok(provider) => {
                println!("  ✓ Provider created successfully");
                println!();

                // Try to initialize the model
                println!("Attempting to load model (this may download models)...");
                match provider.embed("test").await {
                    Ok(response) => {
                        println!("  ✓ Model loaded and inference successful");
                        println!("  ✓ Embedding dimensions: {}", response.embedding.len());
                        println!();
                        println!("=== All checks passed! ===");
                    }
                    Err(e) => {
                        println!("  ✗ Model loading/inference failed:");
                        println!("    Error: {:?}", e);
                        println!();
                        println!("=== Diagnostic Information ===");
                        println!("Provider creation succeeded but model loading failed.");
                        println!("This may indicate:");
                        println!("  1. ONNX Runtime DLL loading issue");
                        println!("  2. Model download/access problem");
                        println!("  3. C runtime mismatch during inference");
                        panic!("Model loading failed: {:?}", e);
                    }
                }
            }
            Err(e) => {
                println!("  ✗ Provider creation failed:");
                println!("    Error: {:?}", e);
                println!();
                println!("=== Diagnostic Information ===");
                println!("Provider creation failed. This may indicate:");
                println!("  1. C runtime library mismatch (LNK2038 error)");
                println!("  2. Missing ONNX Runtime DLLs");
                println!("  3. Incompatible dependency versions");
                println!();
                println!("Troubleshooting steps:");
                println!("  1. Clean and rebuild: cargo clean && cargo build");
                println!("  2. Verify .cargo/config.toml has dynamic runtime setting");
                println!("  3. Check that all dependencies use /MD (dynamic runtime)");
                println!("  4. Install Visual C++ Redistributable if missing");
                panic!("Provider creation failed: {:?}", e);
            }
        }
    }

    /// Test that logs dependency versions for debugging
    #[test]
    fn test_log_dependency_versions() {
        println!("=== Dependency Version Information ===");
        println!();
        
        // Read Cargo.lock to get versions
        let cargo_lock = std::path::Path::new("Cargo.lock");
        if cargo_lock.exists() {
            if let Ok(contents) = std::fs::read_to_string(cargo_lock) {
                let lines: Vec<&str> = contents.lines().collect();
                let deps_to_check = ["fastembed", "ort", "ort-sys", "esaxx-rs", "tokenizers"];
                
                for dep_name in &deps_to_check {
                    let search_str = format!("name = \"{}\"", dep_name);
                    for (i, line) in lines.iter().enumerate() {
                        if line.trim() == search_str {
                            // Look for version on next few lines
                            for j in (i + 1)..(i + 5).min(lines.len()) {
                                if lines[j].contains("version") {
                                    println!("  {}: {}", dep_name, lines[j].trim());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        } else {
            println!("  Cargo.lock not found in current directory");
        }
        println!();
    }

    /// Test that checks build configuration
    #[test]
    #[cfg(target_os = "windows")]
    fn test_build_configuration_windows() {
        println!("=== Build Configuration Check ===");
        println!();

        // Check .cargo/config.toml
        let cargo_config = std::path::Path::new(".cargo/config.toml");
        if cargo_config.exists() {
            println!("  ✓ .cargo/config.toml exists");
            if let Ok(contents) = std::fs::read_to_string(cargo_config) {
                if contents.contains("target-feature=-crt-static") {
                    println!("  ✓ Dynamic runtime (MD) configured correctly");
                } else if contents.contains("target-feature=+crt-static") {
                    println!("  ✗ Static runtime (MT) configured - may cause issues with ONNX Runtime");
                } else {
                    println!("  ? Runtime configuration not explicitly set");
                }
            }
        } else {
            println!("  ✗ .cargo/config.toml not found");
            println!("     This may cause C runtime mismatches on Windows");
        }
        println!();
    }
}

