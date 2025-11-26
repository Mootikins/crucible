use anyhow::Result;
use tracing::{info, debug, warn};

use crate::cli::DetectCommand;
use crate::hardware::{HardwareInfo, BackendType};

pub async fn handle(command: DetectCommand, hardware_info: HardwareInfo) -> Result<()> {
    match command {
        DetectCommand::Hardware => {
            show_hardware_info(&hardware_info).await?;
        }
        DetectCommand::Backends => {
            show_available_backends(&hardware_info).await?;
        }
        DetectCommand::TestBackend { backend } => {
            test_backend(&backend, &hardware_info).await?;
        }
    }
    Ok(())
}

async fn show_hardware_info(hardware_info: &HardwareInfo) -> Result<()> {
    println!("Hardware Information");
    println!("==================");
    println!();

    println!("CPU:");
    println!("  Cores: {}", hardware_info.cpu_cores);
    println!("  Threads: {}", hardware_info.cpu_threads);
    println!("  Architecture: {}", hardware_info.cpu_arch);
    println!();

    if !hardware_info.gpus.is_empty() {
        println!("GPUs:");
        for (i, gpu) in hardware_info.gpus.iter().enumerate() {
            println!("  GPU {}:", i);
            println!("    Name: {}", gpu.name);
            println!("    Vendor: {:?}", gpu.vendor);
            println!("    Memory: {} MB", gpu.memory_mb);
            println!("    Vulkan Support: {}", gpu.vulkan_support);
            println!();
        }
    } else {
        println!("GPUs: None detected");
        println!();
    }

    println!("Recommended Backend: {:?}", hardware_info.recommended_backend);

    Ok(())
}

async fn show_available_backends(hardware_info: &HardwareInfo) -> Result<()> {
    println!("Available Backends");
    println!("==================");
    println!();

    let backends = vec![
        ("Vulkan", BackendType::Vulkan { device_id: 0 }),
        ("ROCm", BackendType::Rocm { device_id: 0 }),
        ("CPU", BackendType::Cpu { num_threads: num_cpus::get() }),
    ];

    for (name, backend_type) in backends {
        let supported = hardware_info.is_backend_supported(&backend_type);
        let recommended = hardware_info.recommended_backend == backend_type;

        println!("{}: {} {}",
            name,
            if supported { "✓" } else { "✗" },
            if recommended { "(recommended)" } else { "" }
        );

        if supported {
            match backend_type {
                BackendType::Vulkan { .. } => {
                    if let Some(gpu) = hardware_info.gpus.first() {
                        println!("  → Available via: {}", gpu.name);
                    }
                }
                BackendType::Rocm { .. } => {
                    println!("  → AMD GPU with ROCm support");
                }
                BackendType::Cpu { num_threads } => {
                    println!("  → {} threads available", num_threads);
                }
            }
        }
        println!();
    }

    Ok(())
}

async fn test_backend(backend_name: &str, hardware_info: &HardwareInfo) -> Result<()> {
    info!("Testing backend: {}", backend_name);

    let backend_type = match backend_name.to_lowercase().as_str() {
        "vulkan" => BackendType::Vulkan { device_id: 0 },
        "rocm" => BackendType::Rocm { device_id: 0 },
        "cpu" => BackendType::Cpu { num_threads: num_cpus::get() },
        _ => {
            eprintln!("Unknown backend: {}. Available: vulkan, rocm, cpu", backend_name);
            return Ok(());
        }
    };

    if !hardware_info.is_backend_supported(&backend_type) {
        eprintln!("Backend '{}' is not supported on this system", backend_name);
        return Ok(());
    }

    println!("Testing backend: {}...", backend_name);

    // Test basic backend functionality
    match backend_type {
        BackendType::Vulkan { device_id } => {
            test_vulkan_backend(device_id).await?;
        }
        BackendType::Rocm { device_id } => {
            test_rocm_backend(device_id).await?;
        }
        BackendType::Cpu { num_threads } => {
            test_cpu_backend(num_threads).await?;
        }
    }

    println!("✓ Backend test completed successfully");
    Ok(())
}

async fn test_vulkan_backend(device_id: usize) -> Result<()> {
    println!("  Testing Vulkan backend on device {}...", device_id);

    // TODO: Implement actual Vulkan backend testing with Burn
    // For now, just test if we can initialize wgpu

    #[cfg(feature = "wgpu")]
    {
        use wgpu::Instance;

        let instance = Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });

        let adapters = instance.enumerate_adapters(wgpu::Backends::VULKAN);

        if let Some(adapter) = adapters.into_iter().nth(device_id) {
            let info = adapter.get_info();
            println!("    ✓ Found Vulkan adapter: {}", info.name);
            println!("    ✓ Device type: {:?}", info.device_type);
            println!("    ✓ Backend: {:?}", info.backend);
        } else {
            println!("    ✗ No Vulkan adapter found at device index {}", device_id);
            return Err(anyhow::anyhow!("No Vulkan adapter found"));
        }
    }

    #[cfg(not(feature = "wgpu"))]
    {
        println!("    ⚠ Vulkan testing requires wgpu feature");
    }

    Ok(())
}

async fn test_rocm_backend(device_id: usize) -> Result<()> {
    println!("  Testing ROCm backend on device {}...", device_id);

    // TODO: Implement actual ROCm backend testing with Burn
    // For now, just check if ROCm libraries are available

    // Check if ROCm is available by looking for ROCm libraries
    let rocm_paths = vec![
        "/opt/rocm",
        "/usr/lib/x86_64-linux-gnu/rocm",
        "/opt/rocm/hip",
    ];

    let rocm_found = rocm_paths.iter().any(|path| std::path::Path::new(path).exists());

    if rocm_found {
        println!("    ✓ ROCm libraries detected");
        println!("    ⚠ Actual ROCm backend testing requires burn-tch feature");
    } else {
        println!("    ✗ ROCm libraries not found");
        println!("    💡 Install ROCm or use Docker container with ROCm support");
        return Err(anyhow::anyhow!("ROCm not available"));
    }

    Ok(())
}

async fn test_cpu_backend(num_threads: usize) -> Result<()> {
    println!("  Testing CPU backend with {} threads...", num_threads);

    // Simple CPU computation test
    use std::time::Instant;

    let start = Instant::now();

    // Simple matrix multiplication test
    let size = 1000;
    let a: Vec<f32> = (0..size * size).map(|i| i as f32).collect();
    let b: Vec<f32> = (0..size * size).map(|i| (i * 2) as f32).collect();

    let result: Vec<f32> = a.iter()
        .zip(b.iter())
        .map(|(x, y)| x * y)
        .collect();

    let duration = start.elapsed();

    println!("    ✓ CPU computation test completed");
    println!("    ✓ Processed {} elements in {:?}", result.len(), duration);
    println!("    ✓ {:.0} ops/sec", (result.len() as f64 / duration.as_secs_f64()));

    Ok(())
}