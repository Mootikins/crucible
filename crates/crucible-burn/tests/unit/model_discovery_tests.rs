//! Unit tests for model discovery module

use crucible_burn::models::{ModelInfo, ModelType, ModelFormat, ModelRegistry};
use std::path::PathBuf;
use tempfile::TempDir;
use std::fs;

#[cfg(test)]
mod model_tests {
    use super::*;

    fn create_test_model_structure() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let model_path = temp_dir.path().join("test-model");

        // Create model directory
        fs::create_dir_all(&model_path).unwrap();

        // Create config.json
        let config_content = r#"
{
    "model_type": "embedding",
    "hidden_size": 384,
    "max_position_embeddings": 512,
    "vocab_size": 30522
}
"#;
        fs::write(model_path.join("config.json"), config_content).unwrap();

        // Create tokenizer.json
        let tokenizer_content = r#"
{
    "model": {
        "type": "WordPiece",
        "vocab": {"[PAD]": 0, "[UNK]": 1}
    }
}
"#;
        fs::write(model_path.join("tokenizer.json"), tokenizer_content).unwrap();

        temp_dir
    }

    fn create_test_safetensors_model() -> TempDir {
        let temp_dir = create_test_model_structure();
        let model_path = temp_dir.path().join("test-model");

        // Create model.safetensors file (empty for testing)
        fs::write(model_path.join("model.safetensors"), b"fake_safetensors").unwrap();

        temp_dir
    }

    fn create_test_gguf_model() -> TempDir {
        let temp_dir = create_test_model_structure();
        let model_path = temp_dir.path().join("test-model");

        // Create model.gguf file (empty for testing)
        fs::write(model_path.join("model.gguf"), b"fake_gguf").unwrap();

        temp_dir
    }

    #[test]
    fn test_model_type_dir_name() {
        assert_eq!(ModelType::Embedding.dir_name(), "embeddings");
        assert_eq!(ModelType::Llm.dir_name(), "llm");
    }

    #[test]
    fn test_model_format_display() {
        assert_eq!(format!("{}", ModelFormat::SafeTensors), "SafeTensors");
        assert_eq!(format!("{}", ModelFormat::GGUF), "GGUF");
        assert_eq!(format!("{}", ModelFormat::PyTorch), "PyTorch");
        assert_eq!(format!("{}", ModelFormat::ONNX), "ONNX");
    }

    #[test]
    fn test_model_info_creation() {
        let model_info = ModelInfo::new(
            "test-model".to_string(),
            ModelType::Embedding,
            ModelFormat::SafeTensors,
            PathBuf::from("/test/path"),
        );

        assert_eq!(model_info.name, "test-model");
        assert_eq!(model_info.model_type, ModelType::Embedding);
        assert_eq!(model_info.format, ModelFormat::SafeTensors);
        assert_eq!(model_info.path, PathBuf::from("/test/path"));
        assert!(model_info.config_path.is_none());
        assert!(model_info.tokenizer_path.is_none());
    }

    #[test]
    fn test_model_info_metadata_loading() {
        let temp_dir = create_test_safetensors_model();
        let model_path = temp_dir.path().join("test-model");

        let mut model_info = ModelInfo::new(
            "test-model".to_string(),
            ModelType::Embedding,
            ModelFormat::SafeTensors,
            model_path.clone(),
        );

        // Load metadata
        model_info.load_metadata().unwrap();

        // Check that metadata was loaded
        assert!(model_info.config_path.is_some());
        assert_eq!(model_info.config_path.unwrap(), model_path.join("config.json"));
        assert!(model_info.tokenizer_path.is_some());
        assert_eq!(model_info.tokenizer_path.unwrap(), model_path.join("tokenizer.json"));
        assert_eq!(model_info.dimensions, Some(384));
    }

    #[test]
    fn test_model_completeness_check() {
        let temp_dir = create_test_safetensors_model();
        let model_path = temp_dir.path().join("test-model");

        let mut model_info = ModelInfo::new(
            "test-model".to_string(),
            ModelType::Embedding,
            ModelFormat::SafeTensors,
            model_path.clone(),
        );

        model_info.load_metadata().unwrap();

        // Should be complete
        assert!(model_info.is_complete());
    }

    #[test]
    fn test_safetensors_format_detection() {
        let temp_dir = create_test_safetensors_model();
        let model_path = temp_dir.path().join("test-model");

        let model_info = ModelInfo::new(
            "test-model".to_string(),
            ModelType::Embedding,
            ModelFormat::SafeTensors,
            model_path.clone(),
        );

        // Should detect SafeTensors file
        assert!(model_info.has_model_file());
    }

    #[test]
    fn test_gguf_format_detection() {
        let temp_dir = create_test_gguf_model();
        let model_path = temp_dir.path().join("test-model");

        let model_info = ModelInfo::new(
            "test-model".to_string(),
            ModelType::Embedding,
            ModelFormat::GGUF,
            model_path.clone(),
        );

        // Should detect GGUF file
        assert!(model_info.has_model_file());
    }

    #[test]
    fn test_incomplete_model_detection() {
        let temp_dir = TempDir::new().unwrap();
        let model_path = temp_dir.path().join("incomplete-model");
        fs::create_dir_all(&model_path).unwrap();

        let model_info = ModelInfo::new(
            "incomplete-model".to_string(),
            ModelType::Embedding,
            ModelFormat::SafeTensors,
            model_path.clone(),
        );

        // Should be incomplete (no config, no model files)
        assert!(!model_info.is_complete());
        assert!(!model_info.has_model_file());
    }

    #[tokio::test]
    async fn test_model_registry_creation() {
        let temp_dir = TempDir::new().unwrap();
        let registry = ModelRegistry::new(vec![temp_dir.path().to_path_buf()]).await.unwrap();

        // Should start empty
        assert_eq!(registry.get_all_models().len(), 0);
    }

    #[tokio::test]
    async fn test_model_registry_scanning() {
        let temp_dir = create_test_safetensors_model();
        let search_path = temp_dir.path().to_path_buf();

        let mut registry = ModelRegistry::new(vec![search_path.clone()]).await.unwrap();
        registry.scan_models().await.unwrap();

        // Should find one model
        let models = registry.get_all_models();
        assert_eq!(models.len(), 1);

        let model = models.values().next().unwrap();
        assert_eq!(model.name, "test-model");
        assert_eq!(model.model_type, ModelType::Embedding);
        assert_eq!(model.format, ModelFormat::SafeTensors);
    }

    #[tokio::test]
    async fn test_model_search() {
        let temp_dir = create_test_safetensors_model();
        let search_path = temp_dir.path().to_path_buf();

        let mut registry = ModelRegistry::new(vec![search_path.clone()]).await.unwrap();
        registry.scan_models().await.unwrap();

        // Exact match
        let found = registry.find_model("test-model").await.unwrap();
        assert_eq!(found.name, "test-model");

        // Partial match
        let found = registry.find_model("test").await.unwrap();
        assert_eq!(found.name, "test-model");

        // No match
        let result = registry.find_model("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_model_filtering() {
        let temp_dir1 = create_test_safetensors_model();
        let temp_dir2 = TempDir::new().unwrap();

        // Create second model (LLM)
        let llm_path = temp_dir2.path().join("llm-model");
        fs::create_dir_all(&llm_path).unwrap();

        let config_content = r#"
{
    "model_type": "causal_lm",
    "hidden_size": 768
}
"#;
        fs::write(llm_path.join("config.json"), config_content).unwrap();
        fs::write(llm_path.join("model.gguf"), b"fake_gguf").unwrap();

        let mut registry = ModelRegistry::new(vec![
            temp_dir1.path().to_path_buf(),
            temp_dir2.path().to_path_buf(),
        ]).await.unwrap();
        registry.scan_models().await.unwrap();

        // List all models
        let all_models = registry.list_models(None);
        assert_eq!(all_models.len(), 2);

        // List only embedding models
        let embedding_models = registry.list_models(Some(ModelType::Embedding));
        assert_eq!(embedding_models.len(), 1);

        // List only LLM models
        let llm_models = registry.list_models(Some(ModelType::Llm));
        assert_eq!(llm_models.len(), 1);
    }

    #[tokio::test]
    async fn test_model_rescan() {
        let temp_dir = create_test_safetensors_model();
        let search_path = temp_dir.path().to_path_buf();

        let mut registry = ModelRegistry::new(vec![search_path.clone()]).await.unwrap();
        registry.scan_models().await.unwrap();

        let initial_count = registry.get_all_models().len();

        // Add a new model
        let new_model_path = temp_dir.path().join("new-model");
        fs::create_dir_all(&new_model_path).unwrap();
        fs::write(new_model_path.join("config.json"), "{}").unwrap();
        fs::write(new_model_path.join("model.gguf"), b"fake").unwrap();

        // Rescan should find the new model
        let new_count = registry.rescan().await.unwrap();
        assert_eq!(new_count, initial_count + 1);
    }

    #[test]
    fn test_model_type_from_directory_name() {
        let temp_dir = TempDir::new().unwrap();

        // Create embedding model directory
        let embed_path = temp_dir.path().join("embedding-model");
        fs::create_dir_all(&embed_path).unwrap();
        fs::write(embed_path.join("config.json"), "{}").unwrap();

        // Create LLM model directory
        let llm_path = temp_dir.path().join("llm-model");
        fs::create_dir_all(&llm_path).unwrap();
        fs::write(llm_path.join("config.json"), "{}").unwrap();

        let registry = ModelRegistry {
            models: std::collections::HashMap::new(),
            search_paths: vec![temp_dir.path().to_path_buf()],
        };

        // Test directory name heuristics
        assert_eq!(
            registry.determine_model_type(&embed_path, &ModelFormat::GGUF, &[]).unwrap(),
            ModelType::Embedding
        );

        assert_eq!(
            registry.determine_model_type(&llm_path, &ModelFormat::GGUF, &[]).unwrap(),
            ModelType::Llm
        );
    }
}