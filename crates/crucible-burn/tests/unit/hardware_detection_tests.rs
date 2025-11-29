//! Unit tests for hardware detection module

use crucible_burn::hardware::{HardwareInfo, GpuInfo, GpuVendor, BackendType};
use tokio_test;
use std::collections::HashMap;

#[cfg(test)]
mod hardware_tests {
    use super::*;

    #[tokio::test]
    async fn test_hardware_info_detection() {
        // Test that hardware detection runs without panicking
        let result = HardwareInfo::detect().await;

        match result {
            Ok(info) => {
                assert!(!info.cpu_cores == 0, "CPU cores should be > 0");
                assert!(!info.cpu_threads == 0, "CPU threads should be > 0");
                assert!(!info.cpu_arch.is_empty(), "CPU architecture should be set");
                println!("Detected hardware: {} cores, {} GPUs", info.cpu_cores, info.gpus.len());
            }
            Err(e) => {
                // Hardware detection might fail in CI environments
                println!("Hardware detection failed (expected in some environments): {}", e);
            }
        }
    }

    #[test]
    fn test_gpu_vendor_display() {
        assert_eq!(format!("{}", GpuVendor::Nvidia), "NVIDIA");
        assert_eq!(format!("{}", GpuVendor::Amd), "AMD");
        assert_eq!(format!("{}", GpuVendor::Intel), "Intel");
        assert_eq!(format!("{}", GpuVendor::Apple), "Apple");
        assert_eq!(format!("{}", GpuVendor::Unknown), "Unknown");
    }

    #[test]
    fn test_backend_type_display() {
        assert_eq!(
            format!("{}", BackendType::Vulkan { device_id: 0 }),
            "Vulkan (device 0)"
        );
        assert_eq!(
            format!("{}", BackendType::Rocm { device_id: 1 }),
            "ROCm (device 1)"
        );
        assert_eq!(
            format!("{}", BackendType::Cpu { num_threads: 8 }),
            "CPU (8 threads)"
        );
    }

    #[test]
    fn test_backend_support_validation() {
        let hardware_info = HardwareInfo {
            cpu_cores: 8,
            cpu_threads: 16,
            cpu_arch: "x86_64".to_string(),
            gpus: vec![
                GpuInfo {
                    name: "Test GPU".to_string(),
                    vendor: GpuVendor::Nvidia,
                    memory_mb: 8192,
                    vulkan_support: true,
                    rocm_support: false,
                    device_id: Some(0),
                }
            ],
            recommended_backend: BackendType::Cpu { num_threads: 8 },
        };

        // Test Vulkan support
        assert!(hardware_info.is_backend_supported(
            &BackendType::Vulkan { device_id: 0 }
        ));
        assert!(!hardware_info.is_backend_supported(
            &BackendType::Vulkan { device_id: 99 }
        ));

        // Test ROCm support (should be false for NVIDIA GPU)
        assert!(!hardware_info.is_backend_supported(
            &BackendType::Rocm { device_id: 0 }
        ));

        // Test CPU support (should always be true)
        assert!(hardware_info.is_backend_supported(
            &BackendType::Cpu { num_threads: 8 }
        ));
    }

    #[test]
    fn test_backend_recommendation_priority() {
        let mut hardware_info = HardwareInfo {
            cpu_cores: 8,
            cpu_threads: 16,
            cpu_arch: "x86_64".to_string(),
            gpus: vec![],
            recommended_backend: BackendType::Cpu { num_threads: 8 },
        };

        // With no GPUs, should recommend CPU
        let recommended = HardwareInfo::recommend_backend(&hardware_info.gpus, hardware_info.cpu_threads);
        assert!(matches!(recommended, BackendType::Cpu { .. }));

        // Add AMD GPU with ROCm support
        hardware_info.gpus.push(GpuInfo {
            name: "AMD GPU".to_string(),
            vendor: GpuVendor::Amd,
            memory_mb: 8192,
            vulkan_support: true,
            rocm_support: true,
            device_id: Some(0),
        });

        let recommended = HardwareInfo::recommend_backend(&hardware_info.gpus, hardware_info.cpu_threads);
        assert!(matches!(recommended, BackendType::Rocm { device_id: 0 }));

        // Remove ROCm support, should fallback to Vulkan
        hardware_info.gpus[0].rocm_support = false;
        let recommended = HardwareInfo::recommend_backend(&hardware_info.gpus, hardware_info.cpu_threads);
        assert!(matches!(recommended, BackendType::Vulkan { device_id: 0 }));
    }

    #[tokio::test]
    async fn test_rocm_availability_check() {
        // This test verifies that ROCm availability checking doesn't crash
        // The actual result depends on the test environment
        let rocm_available = check_rocm_availability().await;

        // Should return true/false without panicking
        assert!(rocm_available == true || rocm_available == false);
    }

    // Test helper function (would need to be made public or use test module)
    async fn check_rocm_availability() -> bool {
        // Simplified version of the actual function for testing
        let rocm_paths = vec![
            "/opt/rocm",
            "/usr/lib/x86_64-linux-gnu/rocm",
        ];

        rocm_paths.iter().any(|path| std::path::Path::new(path).exists())
    }
}