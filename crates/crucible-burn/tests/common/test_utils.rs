//! Common test utilities and helpers

use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;
use tempfile::TempDir;
use serde_json::Value;

/// Test configuration builder
#[derive(Debug, Clone)]
pub struct TestConfigBuilder {
    model_dir: Option<PathBuf>,
    backend: Option<String>,
    server_port: Option<u16>,
    model_search_paths: Vec<PathBuf>,
}

impl TestConfigBuilder {
    pub fn new() -> Self {
        Self {
            model_dir: None,
            backend: None,
            server_port: None,
            model_search_paths: Vec::new(),
        }
    }

    pub fn model_dir<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.model_dir = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn backend<S: Into<String>>(mut self, backend: S) -> Self {
        self.backend = Some(backend.into());
        self
    }

    pub fn server_port(mut self, port: u16) -> Self {
        self.server_port = Some(port);
        self
    }

    pub fn add_search_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.model_search_paths.push(path.as_ref().to_path_buf());
        self
    }

    pub fn build(self) -> String {
        let mut config_lines = Vec::new();

        if let Some(model_dir) = self.model_dir {
            config_lines.push(format!("model_dir = \"{}\"", model_dir.display()));
        }

        if let Some(backend) = self.backend {
            if backend == "cpu" {
                config_lines.push("[default_backend]\ncpu = { num_threads = 4 }".to_string());
            } else if backend == "vulkan" {
                config_lines.push("[default_backend]\nvulkan = { device_id = 0 }".to_string());
            } else if backend == "rocm" {
                config_lines.push("[default_backend]\nrocm = { device_id = 0 }".to_string());
            } else {
                config_lines.push("default_backend = \"auto\"".to_string());
            }
        }

        if let Some(port) = self.server_port {
            config_lines.push(format!("\n[server]\nport = {}", port));
        }

        if !self.model_search_paths.is_empty() {
            config_lines.push("\nmodel_search_paths = [".to_string());
            for path in self.model_search_paths {
                config_lines.push(format!("  \"{}\",", path.display()));
            }
            config_lines.push("]".to_string());
        }

        config_lines.join("\n")
    }

    pub fn write_to_file<P: AsRef<Path>>(self, path: P) -> Result<(), std::io::Error> {
        let config_content = self.build();
        fs::write(path, config_content)
    }
}

/// Model test data builder
pub struct ModelTestBuilder {
    temp_dir: TempDir,
    models: HashMap<String, ModelSpec>,
}

#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub model_type: String,
    pub format: String,
    pub dimensions: Option<usize>,
    pub parameters: Option<u64>,
    pub additional_files: Vec<String>,
}

impl ModelTestBuilder {
    pub fn new() -> Result<Self, std::io::Error> {
        Ok(Self {
            temp_dir: TempDir::new()?,
            models: HashMap::new(),
        })
    }

    pub fn add_model<S: Into<String>>(&mut self, name: S, spec: ModelSpec) -> &mut Self {
        let name = name.into();
        self.models.insert(name, spec);
        self
    }

    pub fn add_embedding_model<S: Into<String>>(&mut self, name: S, format: &str, dimensions: usize) -> &mut Self {
        self.add_model(name, ModelSpec {
            model_type: "embedding".to_string(),
            format: format.to_string(),
            dimensions: Some(dimensions),
            parameters: None,
            additional_files: vec![
                "tokenizer.json".to_string(),
                "config.json".to_string(),
            ],
        })
    }

    pub fn add_llm_model<S: Into<String>>(&mut self, name: S, format: &str, parameters: u64) -> &mut Self {
        self.add_model(name, ModelSpec {
            model_type: "causal_lm".to_string(),
            format: format.to_string(),
            dimensions: None,
            parameters: Some(parameters),
            additional_files: vec![
                "tokenizer.json".to_string(),
                "config.json".to_string(),
            ],
        })
    }

    pub fn build(self) -> Result<TempDir, std::io::Error> {
        for (model_name, spec) in self.models {
            let model_path = self.temp_dir.path().join(&model_name);
            fs::create_dir_all(&model_path)?;

            // Create config.json
            let config = self.create_model_config(&spec);
            fs::write(model_path.join("config.json"), config)?;

            // Create tokenizer.json
            let tokenizer = self.create_tokenizer_config();
            fs::write(model_path.join("tokenizer.json"), tokenizer)?;

            // Create model file based on format
            let model_filename = match spec.format.as_str() {
                "safetensors" => "model.safetensors",
                "gguf" => "model.gguf",
                "ggml" => "model.ggml",
                "bin" => "model.bin",
                "pth" => "model.pth",
                "onnx" => "model.onnx",
                "mlx" => "model.mlx",
                _ => "model.bin",
            };

            fs::write(model_path.join(model_filename), b"fake_model_data")?;

            // Create additional files
            for file_name in spec.additional_files.iter() {
                if file_name != "config.json" && file_name != "tokenizer.json" {
                    fs::write(model_path.join(file_name), "{}")?;
                }
            }
        }

        Ok(self.temp_dir)
    }

    fn create_model_config(&self, spec: &ModelSpec) -> String {
        let mut config = serde_json::Map::new();
        config.insert("model_type".to_string(), Value::String(spec.model_type.clone()));

        if let Some(dimensions) = spec.dimensions {
            config.insert("hidden_size".to_string(), Value::Number(dimensions.into()));
            config.insert("dim".to_string(), Value::Number(dimensions.into()));
        }

        if let Some(parameters) = spec.parameters {
            config.insert("num_parameters".to_string(), Value::Number(parameters.into()));
        }

        // Add common model fields
        config.insert("max_position_embeddings".to_string(), Value::Number(512.into()));
        config.insert("vocab_size".to_string(), Value::Number(30522.into()));

        serde_json::to_string_pretty(&Value::Object(config)).unwrap()
    }

    fn create_tokenizer_config(&self) -> String {
        let config = serde_json::json!({
            "model": {
                "type": "WordPiece",
                "vocab": {
                    "[PAD]": 0,
                    "[UNK]": 1,
                    "[CLS]": 2,
                    "[SEP]": 3,
                    "[MASK]": 4
                }
            }
        });

        serde_json::to_string_pretty(&config).unwrap()
    }

    pub fn get_models_dir(&self) -> PathBuf {
        self.temp_dir.path().to_path_buf()
    }
}

/// Performance test utilities
pub struct PerfTestUtils;

impl PerfTestUtils {
    /// Measure execution time of a function
    pub fn measure_time<F, R>(f: F) -> (R, std::time::Duration)
    where
        F: FnOnce() -> R,
    {
        let start = std::time::Instant::now();
        let result = f();
        let duration = start.elapsed();
        (result, duration)
    }

    /// Measure memory usage (Linux specific)
    #[cfg(target_os = "linux")]
    pub fn measure_memory_usage() -> Result<usize, std::io::Error> {
        let status = fs::read_to_string("/proc/self/status")?;
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(kb_str) = parts.get(1) {
                    let kb: usize = kb_str.parse().unwrap_or(0);
                    return Ok(kb * 1024); // Convert to bytes
                }
            }
        }
        Ok(0)
    }

    /// Measure memory usage (placeholder for non-Linux)
    #[cfg(not(target_os = "linux"))]
    pub fn measure_memory_usage() -> Result<usize, std::io::Error> {
        Ok(0) // Placeholder
    }
}

/// Mock hardware detector for testing
pub struct MockHardwareDetector {
    cpu_cores: usize,
    cpu_threads: usize,
    gpus: Vec<crate::hardware::GpuInfo>,
    recommended_backend: crate::hardware::BackendType,
}

impl MockHardwareDetector {
    pub fn new() -> Self {
        Self {
            cpu_cores: 8,
            cpu_threads: 16,
            gpus: Vec::new(),
            recommended_backend: crate::hardware::BackendType::Cpu { num_threads: 8 },
        }
    }

    pub fn with_cpu(mut self, cores: usize, threads: usize) -> Self {
        self.cpu_cores = cores;
        self.cpu_threads = threads;
        self.recommended_backend = crate::hardware::BackendType::Cpu { num_threads: cores };
        self
    }

    pub fn with_gpu(mut self, name: &str, vendor: crate::hardware::GpuVendor, memory_mb: u64) -> Self {
        self.gpus.push(crate::hardware::GpuInfo {
            name: name.to_string(),
            vendor,
            memory_mb,
            vulkan_support: true,
            rocm_support: matches!(vendor, crate::hardware::GpuVendor::Amd),
            device_id: Some(self.gpus.len() as u32),
        });
        self
    }

    pub fn build(self) -> crate::hardware::HardwareInfo {
        crate::hardware::HardwareInfo {
            cpu_cores: self.cpu_cores,
            cpu_threads: self.cpu_threads,
            cpu_arch: "x86_64".to_string(),
            gpus: self.gpus,
            recommended_backend: self.recommended_backend,
        }
    }
}

/// Test assertion helpers
pub struct TestAssertions;

impl TestAssertions {
    /// Assert that a command succeeded and provide helpful error message
    pub fn assert_command_success(output: &std::process::Output, command_name: &str) {
        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!(
                "Command '{}' failed with exit code {:?}\nSTDOUT:\n{}\nSTDERR:\n{}",
                command_name, output.status.code(), stdout, stderr
            );
        }
    }

    /// Assert that output contains expected text
    pub fn assert_contains<T: AsRef<str>>(output: &str, expected: T) {
        if !output.contains(expected.as_ref()) {
            panic!("Expected output to contain '{}', but it didn't.\nActual output:\n{}",
                   expected.as_ref(), output);
        }
    }

    /// Assert that output contains at least one of several expected texts
    pub fn assert_contains_any<T: AsRef<str>>(output: &str, expected: &[T]) {
        if !expected.iter().any(|e| output.contains(e.as_ref())) {
            let expected_list: Vec<String> = expected.iter().map(|e| e.as_ref().to_string()).collect();
            panic!("Expected output to contain one of {:?}, but it didn't.\nActual output:\n{}",
                   expected_list, output);
        }
    }
}

/// Test data generators
pub struct TestDataGenerators;

impl TestDataGenerators {
    /// Generate test embeddings with deterministic values
    pub fn generate_test_embedding(text: &str, dimensions: usize) -> Vec<f32> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);

        (0..dimensions)
            .map(|i| {
                i.hash(&mut hasher);
                ((hasher.finish() % 1000) as f32 - 500.0) / 1000.0
            })
            .collect()
    }

    /// Generate test texts for embedding testing
    pub fn generate_test_texts() -> Vec<String> {
        vec![
            "Hello, world!".to_string(),
            "This is a test sentence for embedding generation.".to_string(),
            "The quick brown fox jumps over the lazy dog.".to_string(),
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit.".to_string(),
            "Rust is a systems programming language that runs blazingly fast.".to_string(),
        ]
    }

    /// Create a corrupted model file for testing error handling
    pub fn create_corrupted_model_file<P: AsRef<Path>>(path: P) -> Result<(), std::io::Error> {
        let corrupted_data = b"This is not a valid model file format - it's just text that should fail to load properly";
        fs::write(path, corrupted_data)
    }

    /// Create a malformed JSON config for testing error handling
    pub fn create_malformed_config<P: AsRef<Path>>(path: P) -> Result<(), std::io::Error> {
        let malformed_json = r#"
{
    "model_type": "embedding",
    "hidden_size": 384,
    "invalid": "json", structure
    "unclosed_array": [1, 2, 3
"#;
        fs::write(path, malformed_json)
    }
}