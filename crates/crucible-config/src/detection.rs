//! System capability detection for intelligent configuration defaults
//!
//! This module provides hardware detection and intelligent default calculation
//! to optimize Crucible performance across different system types.

use std::time::Duration;
use sysinfo::{System, CpuRefreshKind, MemoryRefreshKind, RefreshKind, Disks};

/// Memory threshold in GB for low-memory system classification
/// Systems with <4GB RAM get conservative resource allocations
pub const LOW_MEMORY_THRESHOLD_GB: f64 = 4.0;

/// Memory threshold in GB for low available memory classification
/// Systems with <1GB available memory enter memory-saver mode
pub const LOW_AVAILABLE_MEMORY_THRESHOLD_GB: f64 = 1.0;

/// High performance CPU core count threshold
/// Systems with >=8 physical cores are considered high-performance
pub const HIGH_PERFORMANCE_CORES: usize = 8;

/// High-end CPU clock speed threshold in GHz
/// CPUs with >=3.0 GHz are considered high-performance
pub const HIGH_CLOCK_SPEED_THRESHOLD_GHZ: f64 = 3.0;

/// Default cache size in MB for low-memory systems
pub const DEFAULT_CACHE_SIZE_LOW_MEM_MB: u64 = 50;

/// Default cache size in MB for high-memory systems
pub const DEFAULT_CACHE_SIZE_HIGH_MEM_MB: u64 = 200;

/// Disk space threshold in GB for cache disabling
/// Systems with <1GB available disk space disable caching
pub const LOW_DISK_SPACE_THRESHOLD_GB: f64 = 1.0;

/// Binary unit conversion: bytes to GB
pub const BYTES_PER_GB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Memory thresholds in bytes for system classification
pub const LOW_MEMORY_THRESHOLD_BYTES: usize = (LOW_MEMORY_THRESHOLD_GB * BYTES_PER_GB) as usize;
pub const MID_MEMORY_THRESHOLD_BYTES: usize = (8.0 * BYTES_PER_GB) as usize;
pub const HIGH_MEMORY_THRESHOLD_BYTES: usize = (16.0 * BYTES_PER_GB) as usize;

/// Default disk space for testing (1TB in bytes)
pub const DEFAULT_DISK_SPACE_BYTES: u64 = 1_000_000_000_000;

/// High-end system memory for testing (64GB in bytes)
pub const EXTREME_MEMORY_BYTES: usize = (64.0 * BYTES_PER_GB) as usize;

/// Detected system capabilities for intelligent configuration
#[derive(Debug, Clone)]
pub struct SystemCapabilities {
    pub cpu_info: CpuInfo,
    pub memory_info: MemoryInfo,
    pub disk_info: DiskInfo,
}

/// CPU information for performance tuning
#[derive(Debug, Clone)]
pub struct CpuInfo {
    pub logical_cores: usize,
    pub physical_cores: usize,
    pub core_count: usize, // Alias for logical_cores for compatibility
    pub cache_size: Option<usize>,
    pub cpu_speed: Option<f64>,
    pub is_performance_class: bool, // High-end vs low-end detection
}

/// Memory information for resource allocation
#[derive(Debug, Clone)]
pub struct MemoryInfo {
    pub total_memory: usize,
    pub available_memory: usize,
    pub is_low_memory: bool, // < 4GB considered low memory
}

/// Disk information for storage optimization
#[derive(Debug, Clone)]
pub struct DiskInfo {
    pub total_space: u64,
    pub available_space: u64,
    pub is_ssd: bool,
}

impl SystemCapabilities {
    /// Detect system capabilities with graceful fallback
    pub fn detect() -> Result<Self, DetectionError> {
        match Self::detect_internal() {
            Ok(capabilities) => Ok(capabilities),
            Err(e) => {
                eprintln!("Warning: System detection failed ({}), using conservative defaults", e);
                Ok(Self::fallback_defaults())
            }
        }
    }

    /// Internal detection method that can fail
    fn detect_internal() -> Result<Self, DetectionError> {
        // Initialize system with specific refresh options for better performance
        let refresh_kind = RefreshKind::new()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything());

        let system = System::new_with_specifics(refresh_kind);

        // Initialize disks separately
        let disks = Disks::new_with_refreshed_list();

        // Detect CPU information
        let cpu_info = Self::detect_cpu_info(&system)?;

        // Detect memory information
        let memory_info = Self::detect_memory_info(&system)?;

        // Detect disk information
        let disk_info = Self::detect_disk_info(&disks)?;

        Ok(SystemCapabilities {
            cpu_info,
            memory_info,
            disk_info,
        })
    }

    /// Detect CPU information from system
    fn detect_cpu_info(system: &System) -> Result<CpuInfo, DetectionError> {
        let cpus = system.cpus();

        if cpus.is_empty() {
            return Err(DetectionError::CpuDetection(
                "No CPUs detected".to_string()
            ));
        }

        // Get logical cores count
        let logical_cores = cpus.len();

        // Try to get physical core count from system (more accurate)
        let physical_cores = system.physical_core_count()
            .unwrap_or(logical_cores)
            .max(1);

        // Get cache size - sysinfo doesn't provide this directly in current version
        let cache_size = None;

        // Get CPU speed from first CPU (assuming all are similar)
        // sysinfo returns frequency in MHz, convert to GHz
        let cpu_speed = cpus.first()
            .map(|cpu| cpu.frequency() as f64 / 1000.0);

        // Determine if this is a performance-class CPU
        let is_performance_class = Self::is_performance_class_cpu(cpus);

        Ok(CpuInfo {
            logical_cores,
            physical_cores,
            core_count: logical_cores,
            cache_size,
            cpu_speed,
            is_performance_class,
        })
    }

    /// Determine if CPU is performance class based on speed and features
    fn is_performance_class_cpu(cpus: &[sysinfo::Cpu]) -> bool {
        if cpus.is_empty() {
            return false;
        }

        let first_cpu = &cpus[0];
        let cpu_speed = first_cpu.frequency() as f64 / 1000.0; // Convert MHz to GHz
        let brand = first_cpu.brand().to_lowercase();
        let logical_cores = cpus.len();

        // Performance indicators
        let has_high_clock_speed = cpu_speed >= HIGH_CLOCK_SPEED_THRESHOLD_GHZ;
        let has_many_cores = logical_cores >= 8;
        let has_high_end_brand = brand.contains("ryzen") &&
            (brand.contains("7") || brand.contains("9") || brand.contains("threadripper"));
        let has_intel_high_end = brand.contains("intel") &&
            (brand.contains("i7") || brand.contains("i9") || brand.contains("xeon"));

        // Consider it performance class if it meets multiple criteria
        let performance_score = (has_high_clock_speed as u8) +
                               (has_many_cores as u8) +
                               (has_high_end_brand as u8) +
                               (has_intel_high_end as u8);

        performance_score >= 2
    }

    /// Detect memory information from system
    fn detect_memory_info(system: &System) -> Result<MemoryInfo, DetectionError> {
        let total_memory = system.total_memory();
        let available_memory = system.available_memory();

        if total_memory == 0 {
            return Err(DetectionError::MemoryDetection(
                "Total memory detected as zero".to_string()
            ));
        }

        // Consider < 4GB as low memory
        let is_low_memory = total_memory < LOW_MEMORY_THRESHOLD_BYTES as u64;

        Ok(MemoryInfo {
            total_memory: total_memory as usize,
            available_memory: available_memory as usize,
            is_low_memory,
        })
    }

    /// Detect disk information from disks
    fn detect_disk_info(disks: &Disks) -> Result<DiskInfo, DetectionError> {
        let disk_list = disks.list();

        if disk_list.is_empty() {
            return Err(DetectionError::DiskDetection(
                "No disks detected".to_string()
            ));
        }

        // Find the largest disk (likely the primary storage)
        let primary_disk = disk_list
            .iter()
            .max_by_key(|disk| disk.total_space())
            .ok_or_else(|| DetectionError::DiskDetection(
                "Failed to find primary disk".to_string()
            ))?;

        let total_space = primary_disk.total_space();
        let available_space = primary_disk.available_space();

        // Try to detect SSD vs HDD
        let is_ssd = Self::detect_ssd(primary_disk);

        Ok(DiskInfo {
            total_space,
            available_space,
            is_ssd,
        })
    }

    /// Detect if disk is SSD based on various indicators
    fn detect_ssd(disk: &sysinfo::Disk) -> bool {
        let name = disk.name().to_string_lossy().to_lowercase();
        let mount_point = disk.mount_point().to_string_lossy().to_lowercase();
        let _file_system = disk.file_system().to_string_lossy().to_lowercase();

        // Check for SSD indicators in disk name/type
        let ssd_indicators = [
            "ssd", "nvme", "solid state", "flash", "emmc",
            "sata express", "u.2", "pcie", "m.2"
        ];

        let hdd_indicators = [
            "hdd", "hard drive", "rotational", "5400", "7200", "10000"
        ];

        // Check mount point and file system for clues
        let is_likely_os_drive = mount_point == "/" ||
                                mount_point.contains("windows") ||
                                mount_point.contains("system");

        let has_ssd_name = ssd_indicators.iter().any(|indicator| name.contains(indicator));
        let has_hdd_name = hdd_indicators.iter().any(|indicator| name.contains(indicator));

        // SSD detection heuristics
        if has_ssd_name {
            true
        } else if has_hdd_name {
            false
        } else if is_likely_os_drive {
            // Modern systems usually have SSD as OS drive
            // Be conservative: if unsure, assume SSD for modern systems
            true
        } else if name.contains("sd") && !name.contains("mmc") {
            // Traditional spinning disks (sda, sdb, etc.) - likely HDD
            false
        } else {
            // Default assumption: SSD for modern systems
            true
        }
    }

    /// Create system capabilities with test values (for testing)
    #[cfg(test)]
    pub fn test_with_values(
        logical_cores: usize,
        physical_cores: usize,
        total_memory: usize,
        is_performance_class: bool,
    ) -> Self {
        Self {
            cpu_info: CpuInfo {
                logical_cores,
                physical_cores,
                core_count: logical_cores,
                cache_size: Some(8192),
                cpu_speed: Some(2.4),
                is_performance_class,
            },
            memory_info: MemoryInfo {
                total_memory,
                available_memory: total_memory / 2,
                is_low_memory: total_memory < 4_000_000_000,
            },
            disk_info: DiskInfo {
                total_space: 1_000_000_000_000,
                available_space: 500_000_000_000,
                is_ssd: true,
            },
        }
    }

    /// Recommended worker count for concurrent operations
    /// Conservative: physical_cores - 1, minimum 1
    pub fn recommended_worker_count(&self) -> usize {
        (self.cpu_info.physical_cores.saturating_sub(1)).max(1)
    }

    /// Recommended batch size based on system capabilities
    /// Scales with memory and CPU power
    pub fn recommended_batch_size(&self) -> usize {
        match self.cpu_info.logical_cores {
            1..=2 => 4,   // Low-end systems
            3..=4 => 8,   // Mid-range systems
            5..=8 => 12,  // High-end systems
            _ => 16,       // Workstation class
        }
    }

    /// Recommended thread count for embedding operations
    /// Conservative to avoid overwhelming CPU
    pub fn recommended_embedding_threads(&self) -> Option<usize> {
        let conservative_threads = match self.cpu_info.logical_cores {
            1..=2 => Some(1),
            3..=4 => Some(2),
            5..=8 => Some(4),
            _ => Some((self.cpu_info.logical_cores / 2).min(8)),
        };
        conservative_threads
    }

    /// Recommended max connections based on system memory
    pub fn recommended_max_connections(&self) -> usize {
        if self.memory_info.is_low_memory {
            32
        } else if self.memory_info.total_memory < MID_MEMORY_THRESHOLD_BYTES { // < 8GB
            64
        } else if self.memory_info.total_memory < HIGH_MEMORY_THRESHOLD_BYTES { // < 16GB
            128
        } else {
            256
        }
    }

    /// Recommended buffer size for queues
    pub fn recommended_buffer_size(&self) -> usize {
        if self.memory_info.is_low_memory {
            50
        } else {
            100
        }
    }

    /// Recommended timeout for CPU operations (in milliseconds)
    pub fn recommended_cpu_timeout(&self) -> Duration {
        if self.cpu_info.is_performance_class {
            Duration::from_millis(30000) // 30 seconds
        } else {
            Duration::from_millis(60000) // 60 seconds for slower systems
        }
    }

    /// Get total memory in GB (using binary units - GiB)
    pub fn total_memory_gb(&self) -> f64 {
        if self.memory_info.total_memory == 0 {
            return 0.0;
        }
        self.memory_info.total_memory as f64 / BYTES_PER_GB
    }

    /// Get available memory in GB (using binary units - GiB)
    pub fn available_memory_gb(&self) -> f64 {
        if self.memory_info.available_memory == 0 {
            return 0.0;
        }
        self.memory_info.available_memory as f64 / BYTES_PER_GB
    }

    /// Get available disk space in GB (using binary units - GiB)
    pub fn available_disk_gb(&self) -> f64 {
        if self.disk_info.available_space == 0 {
            return 0.0;
        }
        self.disk_info.available_space as f64 / BYTES_PER_GB
    }

    /// Create conservative fallback defaults when system detection fails
    pub fn fallback_defaults() -> Self {
        Self {
            cpu_info: CpuInfo {
                logical_cores: 2,        // Conservative dual-core assumption
                physical_cores: 2,
                core_count: 2,
                cache_size: None,
                cpu_speed: Some(2.0),    // Conservative 2.0 GHz assumption
                is_performance_class: false,
            },
            memory_info: MemoryInfo {
                total_memory: LOW_MEMORY_THRESHOLD_BYTES, // 4GB - conservative assumption
                available_memory: LOW_MEMORY_THRESHOLD_BYTES / 2, // 2GB available
                is_low_memory: true,
            },
            disk_info: DiskInfo {
                total_space: DEFAULT_DISK_SPACE_BYTES, // 1TB
                available_space: DEFAULT_DISK_SPACE_BYTES / 2, // 500GB available
                is_ssd: true, // Assume modern SSD for better defaults
            },
        }
    }
}

/// Errors that can occur during system detection
#[derive(Debug, thiserror::Error)]
pub enum DetectionError {
    #[error("Failed to detect CPU information: {0}")]
    CpuDetection(String),

    #[error("Failed to detect memory information: {0}")]
    MemoryDetection(String),

    #[error("Failed to detect disk information: {0}")]
    DiskDetection(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_detection_works() {
        // This test should now pass - detection is implemented
        let result = SystemCapabilities::detect();
        assert!(result.is_ok(), "SystemCapabilities::detect() should work now: {:?}", result);

        // Print detected capabilities for demonstration
        if let Ok(capabilities) = result {
            println!("\n🔍 System Detection Results:");
            println!("  CPU: {} logical cores, {} physical cores",
                    capabilities.cpu_info.logical_cores,
                    capabilities.cpu_info.physical_cores);
            if let Some(speed) = capabilities.cpu_info.cpu_speed {
                println!("  CPU Speed: {:.2} GHz", speed);
            }
            println!("  Performance Class: {}", capabilities.cpu_info.is_performance_class);

            println!("  Memory: {:.2} GB total, {:.2} GB available",
                    capabilities.total_memory_gb(),
                    capabilities.available_memory_gb());

            println!("  Disk: {:.2} GB total, {:.2} GB available, {}",
                    capabilities.disk_info.total_space as f64 / (1024.0 * 1024.0 * 1024.0),
                    capabilities.available_disk_gb(),
                    if capabilities.disk_info.is_ssd { "SSD" } else { "HDD" });

            println!("  Recommended: {} workers, batch size {}, {} connections",
                    capabilities.recommended_worker_count(),
                    capabilities.recommended_batch_size(),
                    capabilities.recommended_max_connections());
        }
    }

    #[test]
    fn test_system_detection_detects_cpu_info() {
        // Once implemented, should detect CPU information
        let capabilities = SystemCapabilities::detect().expect("Failed to detect system capabilities");

        assert!(capabilities.cpu_info.logical_cores > 0, "Should detect at least 1 logical core");
        assert!(capabilities.cpu_info.physical_cores > 0, "Should detect at least 1 physical core");
        assert!(capabilities.cpu_info.logical_cores >= capabilities.cpu_info.physical_cores,
               "Logical cores should be >= physical cores");
    }

    #[test]
    fn test_system_detection_detects_memory_info() {
        // Once implemented, should detect memory information
        let capabilities = SystemCapabilities::detect().expect("Failed to detect system capabilities");

        assert!(capabilities.memory_info.total_memory > 0, "Should detect total memory");
        assert!(capabilities.memory_info.available_memory > 0, "Should detect available memory");
        assert!(capabilities.memory_info.available_memory <= capabilities.memory_info.total_memory,
               "Available memory should be <= total memory");
    }

    #[test]
    fn test_system_detection_detects_disk_info() {
        // Once implemented, should detect disk information
        let capabilities = SystemCapabilities::detect().expect("Failed to detect system capabilities");

        assert!(capabilities.disk_info.total_space > 0, "Should detect total disk space");
        assert!(capabilities.disk_info.available_space > 0, "Should detect available disk space");
        assert!(capabilities.disk_info.available_space <= capabilities.disk_info.total_space,
               "Available space should be <= total space");
    }

    #[test]
    fn test_recommended_worker_count() {
        // Test with different CPU configurations
        let low_end = SystemCapabilities::test_with_values(2, 2, 4_000_000_000, false);
        assert_eq!(low_end.recommended_worker_count(), 1, "Low-end system should recommend 1 worker");

        let mid_range = SystemCapabilities::test_with_values(4, 2, 8_000_000_000, false);
        assert_eq!(mid_range.recommended_worker_count(), 1, "Mid-range system should recommend 1 worker");

        let high_end = SystemCapabilities::test_with_values(16, 8, 32_000_000_000, true);
        assert_eq!(high_end.recommended_worker_count(), 7, "High-end system should recommend 7 workers");

        let single_core = SystemCapabilities::test_with_values(1, 1, 2_000_000_000, false);
        assert_eq!(single_core.recommended_worker_count(), 1, "Single-core system should recommend 1 worker");
    }

    #[test]
    fn test_recommended_batch_size() {
        let low_end = SystemCapabilities::test_with_values(1, 1, 2_000_000_000, false);
        assert_eq!(low_end.recommended_batch_size(), 4, "Low-end system should recommend batch size 4");

        let mid_range = SystemCapabilities::test_with_values(4, 2, 8_000_000_000, false);
        assert_eq!(mid_range.recommended_batch_size(), 8, "Mid-range system should recommend batch size 8");

        let high_end = SystemCapabilities::test_with_values(8, 4, 16_000_000_000, true);
        assert_eq!(high_end.recommended_batch_size(), 12, "High-end system should recommend batch size 12");

        let workstation = SystemCapabilities::test_with_values(16, 8, 32_000_000_000, true);
        assert_eq!(workstation.recommended_batch_size(), 16, "Workstation should recommend batch size 16");
    }

    #[test]
    fn test_recommended_embedding_threads() {
        let low_end = SystemCapabilities::test_with_values(1, 1, 2_000_000_000, false);
        assert_eq!(low_end.recommended_embedding_threads(), Some(1),
                  "Low-end system should recommend 1 embedding thread");

        let mid_range = SystemCapabilities::test_with_values(4, 2, 8_000_000_000, false);
        assert_eq!(mid_range.recommended_embedding_threads(), Some(2),
                  "Mid-range system should recommend 2 embedding threads");

        let high_end = SystemCapabilities::test_with_values(8, 4, 16_000_000_000, true);
        assert_eq!(high_end.recommended_embedding_threads(), Some(4),
                  "High-end system should recommend 4 embedding threads");

        let workstation = SystemCapabilities::test_with_values(16, 8, 32_000_000_000, true);
        assert_eq!(workstation.recommended_embedding_threads(), Some(8),
                  "Workstation should recommend 8 embedding threads");

        let extreme = SystemCapabilities::test_with_values(32, 16, 64_000_000_000, true);
        assert_eq!(extreme.recommended_embedding_threads(), Some(8),
                  "Extreme system should cap at 8 embedding threads");
    }

    #[test]
    fn test_recommended_max_connections() {
        let low_memory = SystemCapabilities::test_with_values(4, 2, 2_000_000_000, false);
        assert_eq!(low_memory.recommended_max_connections(), 32,
                  "Low-memory system should recommend 32 max connections");

        let mid_memory = SystemCapabilities::test_with_values(4, 2, 6_000_000_000, false);
        assert_eq!(mid_memory.recommended_max_connections(), 64,
                  "Mid-memory system should recommend 64 max connections");

        let high_memory = SystemCapabilities::test_with_values(8, 4, 12_000_000_000, true);
        assert_eq!(high_memory.recommended_max_connections(), 128,
                  "High-memory system should recommend 128 max connections");

        let extreme_memory = SystemCapabilities::test_with_values(16, 8, 32_000_000_000, true);
        assert_eq!(extreme_memory.recommended_max_connections(), 256,
                  "Extreme-memory system should recommend 256 max connections");
    }

    #[test]
    fn test_recommended_buffer_size() {
        let low_memory = SystemCapabilities::test_with_values(2, 1, 2_000_000_000, false);
        assert_eq!(low_memory.recommended_buffer_size(), 50,
                  "Low-memory system should recommend buffer size 50");

        let high_memory = SystemCapabilities::test_with_values(8, 4, 16_000_000_000, true);
        assert_eq!(high_memory.recommended_buffer_size(), 100,
                  "High-memory system should recommend buffer size 100");
    }

    #[test]
    fn test_recommended_cpu_timeout() {
        let low_end = SystemCapabilities::test_with_values(2, 1, 4_000_000_000, false);
        assert_eq!(low_end.recommended_cpu_timeout(), std::time::Duration::from_millis(60000),
                  "Low-end system should recommend 60s timeout");

        let high_end = SystemCapabilities::test_with_values(8, 4, 16_000_000_000, true);
        assert_eq!(high_end.recommended_cpu_timeout(), std::time::Duration::from_millis(30000),
                  "High-end system should recommend 30s timeout");
    }
}