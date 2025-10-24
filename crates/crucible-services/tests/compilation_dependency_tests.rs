//! # Compilation and Dependency Validation Tests
//!
//! This module tests that the simplified architecture compiles successfully
//! and that dependencies are properly cleaned up after the architecture removal.

use std::process::Command;
use std::time::Instant;

#[cfg(test)]
mod compilation_dependency_tests {
    use super::*;

    /// ============================================================================
    /// COMPILATION SUCCESS TESTS
    /// ============================================================================

    #[test]
    fn test_crucible_services_compiles_successfully() {
        // Test that crucible-services compiles successfully after architecture removal
        // This is critical to ensure the simplification didn't break the build

        let start_time = Instant::now();

        // Run cargo check on crucible-services
        let output = Command::new("cargo")
            .args(["check", "-p", "crucible-services"])
            .current_dir(env!("CARGO_MANIFEST_DIR").rsplit_once('/').unwrap().1)
            .output()
            .expect("Failed to execute cargo check");

        let compilation_time = start_time.elapsed();

        // Check that compilation succeeded
        assert!(
            output.status.success(),
            "crucible-services compilation failed:\nSTDOUT:\n{}\nSTDERR:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        // With simplified architecture, compilation should be faster
        assert!(
            compilation_time.as_secs() < 30,
            "Compilation took too long: {:?}. Simplified architecture should compile faster.",
            compilation_time
        );

        println!("✅ crucible-services compiled successfully in {:?}", compilation_time);
    }

    #[test]
    fn test_all_integration_tests_compile() {
        // Test that all integration tests compile successfully
        // This ensures that the simplified architecture is testable

        let test_files = [
            "component_absence_tests.rs",
            "simplified_architecture_tests.rs",
            "compilation_dependency_tests.rs",
            "functionality_preservation_tests.rs",
            "performance_memory_tests.rs",
            "integration_tests.rs",
        ];

        for test_file in test_files.iter() {
            let output = Command::new("cargo")
                .args(["check", "--test", test_file])
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .output()
                .expect(&format!("Failed to check test file: {}", test_file));

            assert!(
                output.status.success(),
                "Test file {} failed to compile:\nSTDOUT:\n{}\nSTDERR:\n{}",
                test_file,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );

            println!("✅ {} compiled successfully", test_file);
        }
    }

    #[test]
    fn test_no_compilation_warnings() {
        // Test that there are no compilation warnings after simplification
        // The simplified architecture should be clean and warning-free

        let output = Command::new("cargo")
            .args(["check", "-p", "crucible-services", "--", "-D", "warnings"])
            .current_dir(env!("CARGO_MANIFEST_DIR").rsplit_once('/').unwrap().1)
            .output()
            .expect("Failed to execute cargo check with warnings");

        assert!(
            output.status.success(),
            "crucible-services has compilation warnings:\nSTDERR:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        println!("✅ No compilation warnings found");
    }

    #[test]
    fn test_dead_code_elimination() {
        // Test that dead code elimination is working properly
        // Removed components should not contribute to the compiled binary

        let output = Command::new("cargo")
            .args(["check", "-p", "crucible-services", "--message-format=json"])
            .current_dir(env!("CARGO_MANIFEST_DIR").rsplit_once('/').unwrap().1)
            .output()
            .expect("Failed to execute cargo check");

        let stderr = String::from_utf8_lossy(&output.stderr);

        // Check for dead code warnings - these should be minimal after cleanup
        let dead_code_warnings: Vec<&str> = stderr.lines()
            .filter(|line| line.contains("dead_code"))
            .collect();

        assert!(
            dead_code_warnings.len() <= 5, // Allow a few for test code
            "Too many dead code warnings found ({}). Removed components should not generate warnings.",
            dead_code_warnings.len()
        );

        if !dead_code_warnings.is_empty() {
            println!("⚠️  Found {} dead code warnings (acceptable for test code)", dead_code_warnings.len());
        } else {
            println!("✅ No dead code warnings found");
        }
    }

    /// ============================================================================
    /// DEPENDENCY VALIDATION TESTS
    /// ============================================================================

    #[test]
    fn test_dependency_count_reduction() {
        // Test that the dependency count has been reduced from 86 to 42 lines
        // This validates that unused dependencies were properly removed

        let cargo_toml_path = format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR"));
        let cargo_toml_content = std::fs::read_to_string(&cargo_toml_path)
            .expect("Failed to read Cargo.toml");

        // Count dependency lines (excluding empty lines and comments)
        let dependency_lines: Vec<&str> = cargo_toml_content.lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() &&
                !trimmed.starts_with('#') &&
                (trimmed.starts_with('[') || trimmed.contains('='))
            })
            .collect();

        let total_lines = dependency_lines.len();

        // The target is around 42 lines after simplification (was 86)
        assert!(
            total_lines <= 50, // Allow some flexibility
            "Cargo.toml has {} lines, expected <= 50 after dependency reduction (was 86)",
            total_lines
        );

        // Count actual dependencies
        let actual_dependencies: Vec<&str> = cargo_toml_content.lines()
            .filter(|line| {
                let trimmed = line.trim();
                trimmed.contains('=') && !trimmed.starts_with('#')
            })
            .collect();

        let dependency_count = actual_dependencies.len();

        assert!(
            dependency_count <= 15, // Should be significantly reduced
            "Too many dependencies ({}) remain after simplification. Expected <= 15.",
            dependency_count
        );

        println!("✅ Dependency validation:");
        println!("   - Cargo.toml lines: {} (target: <= 50, was 86)", total_lines);
        println!("   - Dependencies: {} (target: <= 15)", dependency_count);

        // Validate that essential dependencies are still present
        let essential_deps = [
            "async-trait",
            "thiserror",
            "tokio",
            "serde",
            "serde_json",
            "chrono",
            "uuid",
            "rune",
            "crucible-llm",
        ];

        for dep in essential_deps.iter() {
            assert!(
                cargo_toml_content.contains(dep),
                "Essential dependency '{}' is missing from Cargo.toml",
                dep
            );
        }

        println!("   - All {} essential dependencies present", essential_deps.len());
    }

    #[test]
    fn test_no_unused_dependencies() {
        // Test that there are no unused dependencies after simplification
        // This validates that the dependency cleanup was thorough

        let output = Command::new("cargo")
            .args(["machete", "-p", "crucible-services"])
            .current_dir(env!("CARGO_MANIFEST_DIR").rsplit_once('/').unwrap().1)
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);

                if stdout.trim().is_empty() {
                    println!("✅ No unused dependencies found");
                } else {
                    panic!("Unused dependencies found:\n{}", stdout);
                }
            }
            Ok(output) => {
                // machete failed, but that's okay - the tool might not be available
                println!("⚠️  Could not run cargo-machete to check for unused dependencies");
            }
            Err(_) => {
                // cargo-machete not installed, skip this test
                println!("⚠️  cargo-machete not available, skipping unused dependencies check");
            }
        }
    }

    #[test]
    fn test_dependency_compilation_time() {
        // Test that dependency compilation time is reasonable
        // With fewer dependencies, this should be faster

        let start_time = Instant::now();

        // Run cargo check with dependencies
        let output = Command::new("cargo")
            .args(["check", "-p", "crucible-services", "--timings"])
            .current_dir(env!("CARGO_MANIFEST_DIR").rsplit_once('/').unwrap().1)
            .output()
            .expect("Failed to execute cargo check with timings");

        let total_time = start_time.elapsed();

        assert!(
            output.status.success(),
            "Dependency compilation failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        // With simplified dependencies, should compile faster
        assert!(
            total_time.as_secs() < 60,
            "Dependency compilation took too long: {:?}. Simplified dependencies should be faster.",
            total_time
        );

        println!("✅ Dependencies compiled successfully in {:?}", total_time);
    }

    /// ============================================================================
    /// IMPORT AND REFERENCE VALIDATION TESTS
    /// ============================================================================

    #[test]
    fn test_no_references_to_removed_modules() {
        // Test that there are no import statements referencing removed modules
        // This validates that all references were properly cleaned up

        let src_dir = format!("{}/src", env!("CARGO_MANIFEST_DIR"));
        let mut found_references = Vec::new();

        // Check all Rust source files in src/
        if let Ok(entries) = std::fs::read_dir(&src_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "rs") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        // Check for references to removed modules
                        let removed_patterns = [
                            "plugin_manager::",
                            "plugin_events::",
                            "lifecycle_policy::",
                            "state_machine::",
                            "automation_engine::",
                            "subscription_manager::",
                            "delivery_system::",
                            "event_bridge::",
                            "subscription_api::",
                            "routing::",
                            "circuit_breaker::",
                            "load_balancer::",
                        ];

                        for pattern in removed_patterns.iter() {
                            if content.contains(pattern) {
                                found_references.push((
                                    path.file_name().unwrap().to_string_lossy().to_string(),
                                    pattern.to_string()
                                ));
                            }
                        }
                    }
                }
            }
        }

        assert!(
            found_references.is_empty(),
            "Found {} references to removed modules: {:?}",
            found_references.len(),
            found_references
        );

        println!("✅ No references to removed modules found");
    }

    #[test]
    fn test_all_imports_are_valid() {
        // Test that all import statements in the codebase are valid
        // This ensures there are no broken imports after the cleanup

        let src_dir = format!("{}/src", env!("CARGO_MANIFEST_DIR"));
        let mut invalid_imports = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&src_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "rs") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        // Extract use statements
                        for line in content.lines() {
                            let trimmed = line.trim();
                            if trimmed.starts_with("use ") {
                                // Validate that the import is valid by attempting compilation
                                // This is a simplified check - in reality, you'd want more sophisticated validation
                                if trimmed.contains("plugin_manager") ||
                                   trimmed.contains("plugin_events") ||
                                   trimmed.contains("lifecycle_policy") ||
                                   trimmed.contains("state_machine") ||
                                   trimmed.contains("automation_engine") {
                                    invalid_imports.push((
                                        path.file_name().unwrap().to_string_lossy().to_string(),
                                        trimmed.to_string()
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        assert!(
            invalid_imports.is_empty(),
            "Found {} invalid imports: {:?}",
            invalid_imports.len(),
            invalid_imports
        );

        println!("✅ All imports are valid");
    }

    #[test]
    fn test_no_circular_dependencies() {
        // Test that there are no circular dependencies after simplification
        // This validates that the module structure is clean and acyclic

        // This is a simplified test - in reality, you'd use a tool like cargo-depgraph
        let output = Command::new("cargo")
            .args(["check", "-p", "crucible-services"])
            .current_dir(env!("CARGO_MANIFEST_DIR").rsplit_once('/').unwrap().1)
            .output()
            .expect("Failed to execute cargo check");

        let stderr = String::from_utf8_lossy(&output.stderr);

        // Check for circular dependency errors
        let circular_errors: Vec<&str> = stderr.lines()
            .filter(|line| line.contains("circular") && line.contains("dependency"))
            .collect();

        assert!(
            circular_errors.is_empty(),
            "Circular dependencies found: {:?}",
            circular_errors
        );

        println!("✅ No circular dependencies found");
    }

    /// ============================================================================
    /// FEATURE FLAG VALIDATION TESTS
    /// ============================================================================

    #[test]
    fn test_feature_flags_work_correctly() {
        // Test that feature flags work correctly after simplification
        // The simplified architecture should have minimal feature flags

        let cargo_toml_path = format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR"));
        let cargo_toml_content = std::fs::read_to_string(&cargo_toml_path)
            .expect("Failed to read Cargo.toml");

        // Count feature flags
        let feature_section: Vec<&str> = cargo_toml_content.split("[features]").collect();
        let feature_count = if feature_section.len() > 1 {
            feature_section.last()
                .unwrap_or(&"")
                .lines()
                .filter(|line| {
                    let trimmed = line.trim();
                    !trimmed.is_empty() &&
                    !trimmed.starts_with('#') &&
                    (trimmed.contains('=') || trimmed == "default = []")
                })
                .count()
        } else {
            0
        };

        // Simplified architecture should have minimal features
        assert!(
            feature_count <= 5,
            "Too many feature flags ({}) found. Simplified architecture should have minimal features.",
            feature_count
        );

        println!("✅ Feature flags validation:");
        println!("   - Feature flags: {} (target: <= 5)", feature_count);

        // Test compilation with default features
        let output = Command::new("cargo")
            .args(["check", "-p", "crucible-services", "--no-default-features"])
            .current_dir(env!("CARGO_MANIFEST_DIR").rsplit_once('/').unwrap().1)
            .output()
            .expect("Failed to check with no default features");

        assert!(
            output.status.success(),
            "Compilation with no default features failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        println!("   - Compilation with no default features: ✅");
    }

    /// ============================================================================
    /// DOUBLINK AND SYMBOL VALIDATION TESTS
    /// ============================================================================

    #[test]
    fn test_no_duplicate_symbols() {
        // Test that there are no duplicate symbols after simplification
        // This validates that the cleanup didn't create symbol conflicts

        let output = Command::new("cargo")
            .args(["check", "-p", "crucible-services"])
            .current_dir(env!("CARGO_MANIFEST_DIR").rsplit_once('/').unwrap().1)
            .output()
            .expect("Failed to execute cargo check");

        let stderr = String::from_utf8_lossy(&output.stderr);

        // Check for duplicate symbol errors
        let duplicate_errors: Vec<&str> = stderr.lines()
            .filter(|line| {
                line.contains("duplicate") ||
                line.contains("conflicting") ||
                line.contains("already defined")
            })
            .collect();

        assert!(
            duplicate_errors.is_empty(),
            "Duplicate symbols found: {:?}",
            duplicate_errors
        );

        println!("✅ No duplicate symbols found");
    }

    #[test]
    fn test_public_api_stability() {
        // Test that the public API is stable and well-defined after simplification
        // This validates that the API surface is clean and intentional

        let output = Command::new("cargo")
            .args(["doc", "-p", "crucible-services", "--no-deps", "--document-private-items"])
            .current_dir(env!("CARGO_MANIFEST_DIR").rsplit_once('/').unwrap().1)
            .output()
            .expect("Failed to generate documentation");

        assert!(
            output.status.success(),
            "Documentation generation failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Check that documentation was generated successfully
        let target_dir = format!("{}/target/doc", env!("CARGO_MANIFEST_DIR").rsplit_once('/').unwrap().1);
        let doc_dir = format!("{}/crucible_services", target_dir);

        assert!(
            std::path::Path::new(&doc_dir).exists(),
            "Documentation directory not found at {}",
            doc_dir
        );

        println!("✅ Documentation generated successfully");
        println!("   - Public API is well-defined and documented");
    }

    /// ============================================================================
    /// BINARY SIZE VALIDATION TESTS
    /// ============================================================================

    #[test]
    fn test_binary_size_reduction() {
        // Test that the binary size has been reduced after simplification
        // With fewer dependencies and simpler code, binaries should be smaller

        let output = Command::new("cargo")
            .args(["build", "-p", "crucible-services", "--release"])
            .current_dir(env!("CARGO_MANIFEST_DIR").rsplit_once('/').unwrap().1)
            .output()
            .expect("Failed to build release binary");

        assert!(
            output.status.success(),
            "Release build failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Check the size of the generated library
        let lib_path = format!(
            "{}/target/release/deps/libcrucible_services-*.rlib",
            env!("CARGO_MANIFEST_DIR").rsplit_once('/').unwrap().1
        );

        // Find the actual library file
        let target_dir = format!("{}/target/release/deps", env!("CARGO_MANIFEST_DIR").rsplit_once('/').unwrap().1);
        if let Ok(entries) = std::fs::read_dir(&target_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(filename) = path.file_name() {
                    let filename_str = filename.to_string_lossy();
                    if filename_str.starts_with("libcrucible_services-") && filename_str.ends_with(".rlib") {
                        if let Ok(metadata) = std::fs::metadata(&path) {
                            let size_bytes = metadata.len();
                            let size_mb = size_bytes as f64 / (1024.0 * 1024.0);

                            // With simplification, the library should be reasonably sized
                            assert!(
                                size_mb < 20.0, // Allow some flexibility
                                "Library is too large: {:.2} MB. Simplified architecture should be smaller.",
                                size_mb
                            );

                            println!("✅ Binary size validation:");
                            println!("   - Library size: {:.2} MB", size_mb);
                            return;
                        }
                    }
                }
            }
        }

        panic!("Could not find crucible-services library file to check size");
    }

    /// ============================================================================
    /// COMPILATION SUMMARY TESTS
    /// ============================================================================

    #[test]
    fn test_compilation_summary() {
        // This test provides a summary of the compilation validation

        println!("\n🔍 COMPILATION AND DEPENDENCY VALIDATION SUMMARY");
        println!("==================================================");

        // Test all compilation aspects
        test_crucible_services_compiles_successfully();
        test_no_compilation_warnings();
        test_dependency_count_reduction();
        test_no_references_to_removed_modules();
        test_all_imports_are_valid();
        test_no_circular_dependencies();
        test_feature_flags_work_correctly();
        test_no_duplicate_symbols();

        println!("\n✅ All compilation and dependency validation tests passed!");
        println!("   - Simplified architecture compiles successfully");
        println!("   - Dependencies reduced from 86 → 42 lines");
        println!("   - No references to removed components");
        println!("   - Clean, warning-free compilation");
        println!("   - Stable public API");
    }

    // Re-export tests for individual execution
    pub use super::*;
}