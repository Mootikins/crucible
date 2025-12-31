//! Model size detection and classification

use regex::Regex;

/// Model size categories for prompt optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSize {
    /// < 4B parameters - needs explicit tool guidance
    Small,
    /// 4-30B parameters - standard prompting
    Medium,
    /// > 30B parameters - minimal prompting needed
    Large,
}

impl ModelSize {
    /// Detect model size from model name string
    ///
    /// Parses patterns like "granite-3b", "qwen3-4b", "llama-70b"
    pub fn from_model_name(name: &str) -> Self {
        let re = Regex::new(r"(\d+)[bB]").unwrap();
        if let Some(caps) = re.captures(name) {
            let size: u32 = caps[1].parse().unwrap_or(0);
            match size {
                0..=3 => ModelSize::Small,
                4..=30 => ModelSize::Medium,
                _ => ModelSize::Large,
            }
        } else {
            // Default to medium if can't detect
            ModelSize::Medium
        }
    }

    /// Check if this size needs read-only tools
    pub fn is_read_only(&self) -> bool {
        matches!(self, ModelSize::Small)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_models() {
        assert_eq!(ModelSize::from_model_name("granite-micro-3b-q6_k"), ModelSize::Small);
        assert_eq!(ModelSize::from_model_name("phi-2b"), ModelSize::Small);
        assert_eq!(ModelSize::from_model_name("tiny-1B"), ModelSize::Small);
    }

    #[test]
    fn test_medium_models() {
        assert_eq!(ModelSize::from_model_name("qwen3-4b-instruct"), ModelSize::Medium);
        assert_eq!(ModelSize::from_model_name("granite-tiny-7b"), ModelSize::Medium);
        assert_eq!(ModelSize::from_model_name("deepseek-r1-8b"), ModelSize::Medium);
        assert_eq!(ModelSize::from_model_name("qwen3-14b"), ModelSize::Medium);
        assert_eq!(ModelSize::from_model_name("gpt-oss-20b"), ModelSize::Medium);
    }

    #[test]
    fn test_large_models() {
        assert_eq!(ModelSize::from_model_name("qwen2.5-coder-32b"), ModelSize::Large);
        assert_eq!(ModelSize::from_model_name("llama-70b"), ModelSize::Large);
        assert_eq!(ModelSize::from_model_name("gpt-oss-120b"), ModelSize::Large);
    }

    #[test]
    fn test_unknown_defaults_to_medium() {
        assert_eq!(ModelSize::from_model_name("unknown-model"), ModelSize::Medium);
        assert_eq!(ModelSize::from_model_name("gpt-4o"), ModelSize::Medium);
    }

    #[test]
    fn test_is_read_only() {
        assert!(ModelSize::Small.is_read_only());
        assert!(!ModelSize::Medium.is_read_only());
        assert!(!ModelSize::Large.is_read_only());
    }
}
