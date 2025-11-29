use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fmt;
use tracing::{debug, info, warn};

/// GPU vendor types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Unknown,
}

impl fmt::Display for GpuVendor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GpuVendor::Nvidia => write!(f, "NVIDIA"),
            GpuVendor::Amd => write!(f, "AMD"),
            GpuVendor::Intel => write!(f, "Intel"),
            GpuVendor::Apple => write!(f, "Apple"),
            GpuVendor::Unknown => write!(f, "Unknown"),
        }
    }
}

/// GPU information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: GpuVendor,
    pub memory_mb: u64,
    pub vulkan_support: bool,
    pub rocm_support: bool,
    pub device_id: Option<u32>,
}

/// Backend types for computation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackendType {
    Vulkan { device_id: usize },
    Rocm { device_id: usize },
    Cpu { num_threads: usize },
}

impl fmt::Display for BackendType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendType::Vulkan { device_id } => write!(f, "Vulkan (device {})", device_id),
            BackendType::Rocm { device_id } => write!(f, "ROCm (device {})", device_id),
            BackendType::Cpu { num_threads } => write!(f, "CPU ({} threads)", num_threads),
        }
    }
}

/// Hardware information for the current system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub cpu_cores: usize,
    pub cpu_threads: usize,
    pub cpu_arch: String,
    pub gpus: Vec<GpuInfo>,
    pub recommended_backend: BackendType,
}

impl HardwareInfo {
    /// Detect hardware capabilities and recommend optimal backend
    pub async fn detect() -> Result<Self> {
        info!("Starting hardware detection...");

        let cpu_cores = num_cpus::get();
        let cpu_threads = num_cpus::get_physical(); // Note: This might still return logical cores
        let cpu_arch = std::env::consts::ARCH.to_string();

        debug!("CPU: {} cores, {} threads, arch: {}", cpu_cores, cpu_threads, cpu_arch);

        // Detect GPUs
        let mut gpus = Vec::new();

        // Try to detect Vulkan-capable GPUs first
        if let Ok(vulkan_gpus) = detect_vulkan_gpus().await {
            gpus.extend(vulkan_gpus);
        }

        // Try to detect ROCm-capable GPUs
        if let Ok(rocm_gpus) = detect_rocm_gpus().await {
            // Merge with existing GPU info, adding ROCm support
            for rocm_gpu in rocm_gpus {
                if let Some(existing_gpu) = gpus.iter_mut().find(|g| g.name == rocm_gpu.name) {
                    existing_gpu.rocm_support = true;
                } else {
                    gpus.push(rocm_gpu);
                }
            }
        }

        // Recommend backend based on available hardware
        let recommended_backend = Self::recommend_backend(&gpus, cpu_threads);

        let hardware_info = HardwareInfo {
            cpu_cores,
            cpu_threads,
            cpu_arch,
            gpus,
            recommended_backend: recommended_backend.clone(),
        };

        info!("Hardware detection completed. Recommended backend: {}", recommended_backend);

        Ok(hardware_info)
    }

    fn recommend_backend(gpus: &[GpuInfo], cpu_threads: usize) -> BackendType {
        // Priority order: ROCm > Vulkan > CPU

        // Look for AMD GPU with ROCm support
        if let Some(amd_index) = gpus.iter().position(|g| g.vendor == GpuVendor::Amd && g.rocm_support) {
            info!("Recommending ROCm backend for AMD GPU: {}", gpus[amd_index].name);
            return BackendType::Rocm { device_id: amd_index };
        }

        // Look for any GPU with Vulkan support
        if let Some(vulkan_index) = gpus.iter().position(|g| g.vulkan_support) {
            info!("Recommending Vulkan backend for GPU: {}", gpus[vulkan_index].name);
            return BackendType::Vulkan { device_id: vulkan_index };
        }

        // Fallback to CPU
        info!("No GPU acceleration available, recommending CPU backend");
        BackendType::Cpu { num_threads: cpu_threads }
    }

    /// Check if a specific backend type is supported
    pub fn is_backend_supported(&self, backend: &BackendType) -> bool {
        match backend {
            BackendType::Vulkan { device_id } => {
                self.gpus.get(*device_id)
                    .map(|gpu| gpu.vulkan_support)
                    .unwrap_or(false)
            }
            BackendType::Rocm { device_id } => {
                self.gpus.get(*device_id)
                    .map(|gpu| gpu.rocm_support)
                    .unwrap_or(false)
            }
            BackendType::Cpu { .. } => true, // CPU is always supported
        }
    }
}

/// Detect Vulkan-capable GPUs (placeholder implementation)
async fn detect_vulkan_gpus() -> Result<Vec<GpuInfo>> {
    let mut gpus = Vec::new();

    debug!("Detecting Vulkan-capable GPUs (placeholder implementation)...");

    // Placeholder: Check for common GPU detection methods
    // This would be implemented with actual wgpu/Vulkan detection when Burn is integrated

    #[cfg(target_os = "linux")]
    {
        // Try to detect GPUs via lspci or similar methods
        if let Ok(output) = std::process::Command::new("lspci")
            .arg("-nn")
            .output()
        {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if line.contains("VGA") || line.contains("3D") || line.contains("Display") ||
                   line.contains("Display controller") {
                    if line.to_lowercase().contains("nvidia") {
                        gpus.push(GpuInfo {
                            name: "NVIDIA GPU".to_string(),
                            vendor: GpuVendor::Nvidia,
                            memory_mb: 0, // TODO: Parse actual memory
                            vulkan_support: true, // Assume modern NVIDIA GPUs support Vulkan
                            rocm_support: false,
                            device_id: Some(gpus.len() as u32),
                        });
                    } else if line.to_lowercase().contains("amd") || line.to_lowercase().contains("radeon") {
                        gpus.push(GpuInfo {
                            name: "AMD GPU".to_string(),
                            vendor: GpuVendor::Amd,
                            memory_mb: 0, // TODO: Parse actual memory
                            vulkan_support: true, // Assume modern AMD GPUs support Vulkan
                            rocm_support: false, // Will be updated by ROCm detection
                            device_id: Some(gpus.len() as u32),
                        });
                    } else if line.to_lowercase().contains("intel") {
                        gpus.push(GpuInfo {
                            name: "Intel GPU".to_string(),
                            vendor: GpuVendor::Intel,
                            memory_mb: 0, // TODO: Parse actual memory
                            vulkan_support: true, // Assume modern Intel GPUs support Vulkan
                            rocm_support: false,
                            device_id: Some(gpus.len() as u32),
                        });
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        // macOS always has some form of GPU support
        gpus.push(GpuInfo {
            name: "Apple GPU".to_string(),
            vendor: GpuVendor::Apple,
            memory_mb: 0, // TODO: Detect actual memory
            vulkan_support: true, // Metal support means Vulkan likely available
            rocm_support: false,
            device_id: Some(0),
        });
    }

    debug!("Found {} GPU(s) with placeholder detection", gpus.len());
    Ok(gpus)
}

/// Detect ROCm-capable GPUs
async fn detect_rocm_gpus() -> Result<Vec<GpuInfo>> {
    let mut gpus = Vec::new();

    debug!("Detecting ROCm-capable GPUs...");

    // Check if ROCm is available
    let rocm_available = check_rocm_availability().await;

    if !rocm_available {
        debug!("ROCm not available");
        return Ok(gpus);
    }

    // Try to get GPU information through ROCm tools
    if let Ok(rocm_info) = get_rocm_gpu_info().await {
        for gpu_info in rocm_info {
            gpus.push(GpuInfo {
                name: gpu_info.name,
                vendor: GpuVendor::Amd,
                memory_mb: gpu_info.memory_mb,
                vulkan_support: false, // Will be updated by Vulkan detection
                rocm_support: true,
                device_id: gpu_info.device_id,
            });
        }
    }

    Ok(gpus)
}

/// Check if ROCm is available on the system
async fn check_rocm_availability() -> bool {
    // Check for ROCm installation paths
    let rocm_paths = vec![
        "/opt/rocm",
        "/usr/lib/x86_64-linux-gnu/rocm",
        "/opt/rocm/hip",
    ];

    let path_found = rocm_paths.iter().any(|path| std::path::Path::new(path).exists());

    if !path_found {
        return false;
    }

    // Check for ROCm libraries
    let rocm_libs = vec![
        "libhipblas.so",
        "librocblas.so",
        "libMIOpen.so",
    ];

    let lib_found = rocm_libs.iter().any(|lib| {
        std::path::Path::new("/opt/rocm/lib").join(lib).exists() ||
        std::path::Path::new("/usr/lib/x86_64-linux-gnu").join(lib).exists()
    });

    path_found && lib_found
}

/// Get GPU information from ROCm
async fn get_rocm_gpu_info() -> Result<Vec<RocmGpuInfo>> {
    let mut gpus = Vec::new();

    // Try to run rocminfo to get GPU information
    if let Ok(output) = std::process::Command::new("rocminfo")
        .output()
    {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);

            // Parse rocminfo output to extract GPU information
            // This is a simplified parser - a real implementation would be more robust
            let mut current_gpu: Option<RocmGpuInfo> = None;

            for line in output_str.lines() {
                let line = line.trim();

                if line.contains("GPU") && line.contains("ID") {
                    if let Some(gpu) = current_gpu.take() {
                        gpus.push(gpu);
                    }

                    // Extract GPU ID and name
                    // This is simplified - real parsing would be more sophisticated
                    current_gpu = Some(RocmGpuInfo {
                        name: "AMD GPU".to_string(), // Would parse actual name
                        memory_mb: 0, // Would parse actual memory
                        device_id: None,
                    });
                }
            }

            if let Some(gpu) = current_gpu {
                gpus.push(gpu);
            }
        }
    }

    // If rocminfo fails, we could try other ROCm tools
    if gpus.is_empty() {
        // Fallback: assume at least one AMD GPU if ROCm is detected
        warn!("Could not parse ROCm GPU info, assuming AMD GPU is available");
        gpus.push(RocmGpuInfo {
            name: "AMD GPU (ROCm)".to_string(),
            memory_mb: 0,
            device_id: None,
        });
    }

    Ok(gpus)
}

/// Temporary struct for ROCm GPU information
#[derive(Debug)]
struct RocmGpuInfo {
    name: String,
    memory_mb: u64,
    device_id: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hardware_detection() {
        let hardware_info = HardwareInfo::detect().await;

        match hardware_info {
            Ok(info) => {
                println!("Detected {} CPU cores", info.cpu_cores);
                println!("Found {} GPUs", info.gpus.len());
                println!("Recommended backend: {}", info.recommended_backend);
            }
            Err(e) => {
                println!("Hardware detection failed: {}", e);
            }
        }
    }
}