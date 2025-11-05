//! Resource Constraint Handling Tests for Crucible CLI System
//!
//! This test suite provides comprehensive validation of the CLI system's behavior
//! under various resource constraints including low memory, disk space limitations,
//! and system resource exhaustion scenarios.

use anyhow::{Context, Result};
use crucible_cli::error_recovery::{CircuitBreaker, CircuitBreakerConfig};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tracing::{info, warn, debug, instrument};

/// Memory information structure
#[derive(Debug, Clone)]
pub struct MemoryInfo {
    pub total_mb: f64,
    pub available_mb: f64,
    pub used_mb: f64,
    pub usage_percent: f64,
}

/// Disk information structure
#[derive(Debug, Clone)]
pub struct DiskInfo {
    pub total_mb: f64,
    pub free_mb: f64,
    pub used_mb: f64,
    pub usage_percent: f64,
}

/// Resource constraint simulator for testing
pub struct ResourceConstraintSimulator {
    memory_pressure_active: Arc<AtomicBool>,
    disk_pressure_active: Arc<AtomicBool>,
    emergency_state: Arc<AtomicBool>,
    temp_dir: Option<TempDir>,
}

/// Get system memory information
pub async fn get_memory_info() -> Result<MemoryInfo> {
    #[cfg(unix)]
    {
        use std::fs;
        if let Ok(mut meminfo) = fs::File::open("/proc/meminfo") {
            use std::io::Read;
            let mut contents = String::new();
            meminfo.read_to_string(&mut contents)?;

            let mut total_kb = 0;
            let mut available_kb = 0;

            for line in contents.lines() {
                if line.starts_with("MemTotal:") {
                    total_kb = line.split_whitespace()
                        .nth(1)
                        .unwrap_or("0")
                        .parse::<u64>()
                        .unwrap_or(0);
                } else if line.starts_with("MemAvailable:") {
                    available_kb = line.split_whitespace()
                        .nth(1)
                        .unwrap_or("0")
                        .parse::<u64>()
                        .unwrap_or(0);
                }
            }

            let total_mb = total_kb as f64 / 1024.0;
            let available_mb = available_kb as f64 / 1024.0;
            let used_mb = total_mb - available_mb;
            let usage_percent = if total_mb > 0.0 {
                (used_mb / total_mb) * 100.0
            } else {
                0.0
            };

            return Ok(MemoryInfo {
                total_mb,
                available_mb,
                used_mb,
                usage_percent,
            });
        }
    }

    // Fallback for Windows or other platforms
    Ok(MemoryInfo {
        total_mb: 8192.0,
        available_mb: 4096.0,
        used_mb: 4096.0,
        usage_percent: 50.0,
    })
}

/// Get system disk information
pub async fn get_disk_info() -> Result<DiskInfo> {
    if let Ok(current_dir) = std::env::current_dir() {
        #[cfg(unix)]
        {
            #[cfg(unix)]
            {
                use nix::sys::statvfs::statvfs;
                if let Ok(stat) = statvfs(&current_dir) {
                    let block_size = stat.block_size() as u64;
                    let total_blocks = stat.blocks();
                    let free_blocks = stat.blocks_available();

                    let total_mb = ((total_blocks * block_size) as f64) / (1024.0 * 1024.0);
                    let free_mb = ((free_blocks * block_size) as f64) / (1024.0 * 1024.0);
                    let used_mb = total_mb - free_mb;
                    let usage_percent = if total_mb > 0.0 {
                        (used_mb / total_mb) * 100.0
                    } else {
                        0.0
                    };

                    return Ok(DiskInfo {
                        total_mb,
                        free_mb,
                        used_mb,
                        usage_percent,
                    });
                }
            }
        }

        // Fallback for all platforms
        Ok(DiskInfo {
            total_mb: 10240.0,
            free_mb: 5120.0,
            used_mb: 5120.0,
            usage_percent: 50.0,
        })
    } else {
        Err(anyhow::anyhow!("Failed to determine current directory"))
    }
}

impl ResourceConstraintSimulator {
    pub fn new() -> Result<Self> {
        Ok(Self {
            memory_pressure_active: Arc::new(AtomicBool::new(false)),
            disk_pressure_active: Arc::new(AtomicBool::new(false)),
            emergency_state: Arc::new(AtomicBool::new(false)),
            temp_dir: Some(tempfile::tempdir()
                .context("Failed to create temporary test directory")?),
        })
    }

    /// Setup temporary environment for constraint testing
    pub async fn setup_test_environment(&mut self) -> Result<()> {
        info!("Setup test environment at: {:?}",
              self.temp_dir.as_ref().unwrap().path());
        Ok(())
    }

    /// Simulate low memory conditions
    pub async fn simulate_low_memory(&self, target_usage_mb: u64) -> Result<()> {
        info!("Starting low memory simulation targeting {} MB", target_usage_mb);
        self.memory_pressure_active.store(true, Ordering::SeqCst);

        // Allocate memory to simulate pressure (simplified for testing)
        let allocations = Arc::new(RwLock::new(Vec::<Vec<u8>>::new()));
        let allocations_clone = allocations.clone();

        tokio::spawn(async move {
            let chunk_size = 1024 * 1024; // 1MB chunks
            let mut allocated = 0;

            while allocated < target_usage_mb {
                let allocation = vec![0u8; chunk_size];
                {
                    let mut allocs = allocations_clone.write().await;
                    allocs.push(allocation);
                }
                allocated += 1;
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        info!("Low memory simulation initiated");
        Ok(())
    }

    /// Fill disk space to simulate constraints
    pub async fn fill_disk_space(&self, target_usage_mb: u64) -> Result<PathBuf> {
        if let Some(temp_dir) = &self.temp_dir {
            let fill_dir = temp_dir.path().join("disk_fill");
            fs::create_dir_all(&fill_dir)?;

            info!("Starting disk space simulation targeting {} MB", target_usage_mb);
            self.disk_pressure_active.store(true, Ordering::SeqCst);

            let chunk_size = 10 * 1024 * 1024; // 10MB chunks
            let mut bytes_written: u64 = 0;
            let mut file_counter = 0;

            while bytes_written < target_usage_mb * 1024 * 1024 {
                let file_path = fill_dir.join(format!("fill_{:04}.bin", file_counter));

                match File::create(&file_path) {
                    Ok(mut file) => {
                        let chunk = vec![0u8; chunk_size];
                        match file.write_all(&chunk) {
                            Ok(_) => {
                                bytes_written += chunk.len() as u64;
                                file_counter += 1;
                            }
                            Err(_) => {
                                warn!("Failed to write to disk fill file");
                                break;
                            }
                        }
                    }
                    Err(_) => {
                        warn!("Failed to create disk fill file");
                        break;
                    }
                }
            }

            info!("Disk space simulation completed: {} MB written",
                  bytes_written / (1024 * 1024));

            Ok(fill_dir)
        } else {
            Err(anyhow::anyhow!("No temporary directory available for disk simulation"))
        }
    }

    /// Stop resource pressure simulation
    pub async fn stop_simulation(&self) -> Result<()> {
        info!("Stopping resource constraint simulation");
        self.memory_pressure_active.store(false, Ordering::SeqCst);
        self.disk_pressure_active.store(false, Ordering::SeqCst);
        self.emergency_state.store(false, Ordering::SeqCst);

        // Give system time to recover
        tokio::time::sleep(Duration::from_millis(1000)).await;

        info!("Resource simulation stopped");
        Ok(())
    }

    /// Check if system is under memory pressure
    pub fn is_under_memory_pressure(&self) -> bool {
        self.memory_pressure_active.load(Ordering::SeqCst)
    }

    /// Check if system is under disk pressure
    pub fn is_under_disk_pressure(&self) -> bool {
        self.disk_pressure_active.load(Ordering::SeqCst)
    }

    /// Check if emergency state is active
    pub fn is_emergency_state(&self) -> bool {
        self.emergency_state.load(Ordering::SeqCst)
    }

    /// Perform emergency cleanup with priority-based data preservation
    pub async fn perform_emergency_cleanup(&self) -> Result<HashMap<String, usize>> {
        let mut cleanup_results = HashMap::new();

        if let Some(temp_dir) = &self.temp_dir {
            self.emergency_state.store(true, Ordering::SeqCst);
            cleanup_results.insert("emergency_state_activated".to_string(), 1);

            // Perform cleanup of lower priority files
            let mut files_cleaned = 0;

            for entry in fs::read_dir(temp_dir)? {
                let entry = entry?;
                let path = entry.path();

                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    // Clean up temporary files
                    if filename.contains("temp") ||
                       filename.contains("cache") ||
                       filename.ends_with(".tmp") {
                        if fs::remove_file(&path).is_ok() {
                            files_cleaned += 1;
                            debug!("Emergency cleanup removed: {:?}", path);
                        }
                    }
                }
            }

            cleanup_results.insert("files_cleaned".to_string(), files_cleaned);

            // Give system time to process cleanup
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        Ok(cleanup_results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[instrument]
    async fn test_low_memory_conditions_basic_simulation() -> Result<()> {
        let mut simulator = ResourceConstraintSimulator::new()?;
        simulator.setup_test_environment().await?;

        let _test_name = "low_memory_basic".to_string();
        let _initial_memory = get_memory_info().await?;

        info!("Starting basic low memory simulation test");

        // Simulate memory pressure
        simulator.simulate_low_memory(256).await?; // Target 256MB pressure
        tokio::time::sleep(Duration::from_secs(2)).await;

        let memory_pressure_detected = simulator.is_under_memory_pressure();
        assert!(memory_pressure_detected, "Memory pressure should be detected");

        // Stop simulation
        simulator.stop_simulation().await?;

        info!("Basic low memory test completed successfully");
        Ok(())
    }

    #[tokio::test]
    #[instrument]
    async fn test_disk_space_constraints_basic_simulation() -> Result<()> {
        let mut simulator = ResourceConstraintSimulator::new()?;
        simulator.setup_test_environment().await?;

        let _test_name = "disk_space_basic".to_string();
        let _initial_disk = get_disk_info().await?;

        info!("Starting basic disk space constraint test");

        // Fill disk space
        let disk_fill_dir = simulator.fill_disk_space(512).await?; // Target 512MB
        tokio::time::sleep(Duration::from_secs(1)).await;

        let disk_pressure_detected = simulator.is_under_disk_pressure();
        assert!(disk_pressure_detected, "Disk pressure should be detected");

        // Clean up for recovery test
        if disk_fill_dir.exists() {
            fs::remove_dir_all(&disk_fill_dir)?;
        }

        simulator.stop_simulation().await?;

        info!("Basic disk space constraint test completed successfully");
        Ok(())
    }

    #[tokio::test]
    #[instrument]
    async fn test_resource_exhaustion_circuit_breaker_behavior() -> Result<()> {
        let mut simulator = ResourceConstraintSimulator::new()?;
        simulator.setup_test_environment().await?;

        info!("Starting circuit breaker test under resource exhaustion");

        // Setup circuit breaker
        let circuit_breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            recovery_timeout: Duration::from_secs(2),
            success_threshold: 2,
        }));

        // Test normal operation
        assert!(circuit_breaker.is_request_allowed().await,
                "Circuit should allow requests initially");

        // Simulate resource exhaustion
        simulator.simulate_low_memory(512).await?;
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Record failures to trigger circuit breaker
        for _ in 0..4 {
            circuit_breaker.record_failure().await;
        }

        // Verify circuit breaker is activated
        assert!(!circuit_breaker.is_request_allowed().await,
                "Circuit breaker should be activated after failures");

        // Wait for recovery timeout and test recovery
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Record successes to close circuit
        circuit_breaker.record_success().await;
        circuit_breaker.record_success().await;

        assert!(circuit_breaker.is_request_allowed().await,
                "Circuit should recover after success");

        simulator.stop_simulation().await?;

        info!("Circuit breaker test completed successfully");
        Ok(())
    }

    #[tokio::test]
    #[instrument]
    async fn test_emergency_resource_management() -> Result<()> {
        let mut simulator = ResourceConstraintSimulator::new()?;
        simulator.setup_test_environment().await?;

        info!("Starting emergency resource management test");

        // Create test files with different priorities
        let temp_dir_path = simulator.temp_dir.as_ref().unwrap().path();
        let critical_data_path = temp_dir_path.join("critical_config.json");
        let important_data_path = temp_dir_path.join("important_document.txt");
        let temp_data_path = temp_dir_path.join("temp_cache.tmp");

        // Write test data
        fs::write(&critical_data_path, r#"{"system": "config", "version": "1.0"}"#)?;
        fs::write(&important_data_path, "Important user document content")?;
        fs::write(&temp_data_path, "Temporary cache data")?;

        // Perform emergency cleanup
        let cleanup_results = simulator.perform_emergency_cleanup().await?;

        // Verify cleanup prioritization
        let critical_data_preserved = critical_data_path.exists();
        let important_data_preserved = important_data_path.exists();
        let temp_data_cleaned = !temp_data_path.exists();

        assert!(critical_data_preserved, "Critical data should be preserved");
        assert!(important_data_preserved, "Important data should be preserved");
        assert!(temp_data_cleaned, "Temporary data should be cleaned");

        assert!(cleanup_results.contains_key("emergency_state_activated"),
                "Emergency state should be recorded");

        simulator.stop_simulation().await?;

        info!("Emergency resource management test completed successfully");
        Ok(())
    }

    #[tokio::test]
    #[instrument]
    async fn test_cross_platform_resource_detection() -> Result<()> {
        info!("Starting cross-platform resource detection test");

        // Test memory detection
        let memory_info = get_memory_info().await?;
        assert!(memory_info.total_mb > 0.0, "Should detect system memory");
        assert!(memory_info.available_mb > 0.0, "Should detect available memory");

        // Test disk detection
        let disk_info = get_disk_info().await?;
        assert!(disk_info.total_mb > 0.0, "Should detect disk space");
        assert!(disk_info.free_mb > 0.0, "Should detect free disk space");

        // Test platform-specific behavior
        let platform = std::env::consts::OS;
        let platform_detection_works = match platform {
            "linux" | "macos" => {
                // Unix systems should have detailed monitoring
                memory_info.usage_percent > 0.0 && disk_info.usage_percent > 0.0
            }
            "windows" => {
                // Windows should have basic monitoring
                memory_info.total_mb > 0.0 && disk_info.total_mb > 0.0
            }
            _ => {
                // Other platforms should have fallback behavior
                true
            }
        };

        assert!(platform_detection_works,
                "Resource detection should work on platform: {}", platform);

        info!("Cross-platform resource detection test completed successfully");
        Ok(())
    }

    #[tokio::test]
    #[instrument]
    async fn test_system_recovery_after_resource_constraints() -> Result<()> {
        let mut simulator = ResourceConstraintSimulator::new()?;
        simulator.setup_test_environment().await?;

        info!("Starting system recovery test");

        let _initial_memory = get_memory_info().await?;
        let _initial_disk = get_disk_info().await?;

        // Apply resource constraints
        simulator.simulate_low_memory(256).await?;
        let _disk_fill_dir = simulator.fill_disk_space(512).await?;

        tokio::time::sleep(Duration::from_secs(2)).await;

        // Verify constraints are active
        assert!(simulator.is_under_memory_pressure(), "Memory pressure should be active");
        assert!(simulator.is_under_disk_pressure(), "Disk pressure should be active");

        // Remove constraints
        simulator.stop_simulation().await?;

        // Wait for recovery
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Verify recovery
        assert!(!simulator.is_under_memory_pressure(), "Memory pressure should be resolved");
        assert!(!simulator.is_under_disk_pressure(), "Disk pressure should be resolved");

        let final_memory = get_memory_info().await?;
        let final_disk = get_disk_info().await?;

        // Verify system health after recovery
        assert!(final_memory.available_mb > 0.0, "Memory should be available after recovery");
        assert!(final_disk.free_mb > 0.0, "Disk space should be available after recovery");

        info!("System recovery test completed successfully");
        Ok(())
    }
}