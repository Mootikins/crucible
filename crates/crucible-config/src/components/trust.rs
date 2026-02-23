//! Trust levels and data classification for security policies
//!
//! This module defines trust levels and data classification enums that work together
//! to enforce security policies. Trust levels represent the trustworthiness of a system
//! or environment, while data classification represents the sensitivity of data.

use serde::{Deserialize, Serialize};

/// Trust level of a system or environment.
///
/// Trust levels are ordered from most to least trustworthy:
/// `Local > Cloud > Untrusted`
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    /// Untrusted environment (lowest trust)
    Untrusted,
    /// Cloud-based environment (medium trust)
    #[default]
    Cloud,
    /// Local environment (highest trust)
    Local,
}

impl TrustLevel {
    /// Check if this trust level satisfies the requirements of a data classification
    pub fn satisfies(&self, classification: DataClassification) -> bool {
        self >= &classification.required_trust_level()
    }

    /// Get the trust level as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Cloud => "cloud",
            Self::Untrusted => "untrusted",
        }
    }
}

impl std::fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Data classification level indicating sensitivity of data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DataClassification {
    /// Public data (lowest sensitivity)
    #[default]
    Public,
    /// Internal data (medium sensitivity)
    Internal,
    /// Confidential data (highest sensitivity)
    Confidential,
}

impl DataClassification {
    /// Get the minimum trust level required for this classification
    pub fn required_trust_level(&self) -> TrustLevel {
        match self {
            Self::Public => TrustLevel::Untrusted,
            Self::Internal => TrustLevel::Cloud,
            Self::Confidential => TrustLevel::Local,
        }
    }

    /// Return all classification variants.
    ///
    /// Useful for building prompts or UI selectors without hardcoding levels.
    pub fn all() -> &'static [DataClassification] {
        &[
            DataClassification::Public,
            DataClassification::Internal,
            DataClassification::Confidential,
        ]
    }

    /// Parse a classification from a string (case-insensitive).
    pub fn from_str_insensitive(s: &str) -> Option<DataClassification> {
        match s.to_lowercase().as_str() {
            "public" => Some(DataClassification::Public),
            "internal" => Some(DataClassification::Internal),
            "confidential" => Some(DataClassification::Confidential),
            _ => None,
        }
    }

    /// Get the classification as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Confidential => "confidential",
        }
    }
}

impl std::fmt::Display for DataClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== TrustLevel Tests =====

    #[test]
    fn test_trust_level_default() {
        assert_eq!(TrustLevel::default(), TrustLevel::Cloud);
    }

    #[test]
    fn test_trust_level_ordering() {
        // Local > Cloud > Untrusted
        assert!(TrustLevel::Local > TrustLevel::Cloud);
        assert!(TrustLevel::Cloud > TrustLevel::Untrusted);
        assert!(TrustLevel::Local > TrustLevel::Untrusted);
    }

    #[test]
    fn test_trust_level_equality() {
        assert_eq!(TrustLevel::Local, TrustLevel::Local);
        assert_eq!(TrustLevel::Cloud, TrustLevel::Cloud);
        assert_eq!(TrustLevel::Untrusted, TrustLevel::Untrusted);
        assert_ne!(TrustLevel::Local, TrustLevel::Cloud);
    }

    #[test]
    fn test_trust_level_as_str() {
        assert_eq!(TrustLevel::Local.as_str(), "local");
        assert_eq!(TrustLevel::Cloud.as_str(), "cloud");
        assert_eq!(TrustLevel::Untrusted.as_str(), "untrusted");
    }

    #[test]
    fn test_trust_level_display() {
        assert_eq!(TrustLevel::Local.to_string(), "local");
        assert_eq!(TrustLevel::Cloud.to_string(), "cloud");
        assert_eq!(TrustLevel::Untrusted.to_string(), "untrusted");
    }

    // ===== DataClassification Tests =====

    #[test]
    fn test_data_classification_default() {
        assert_eq!(DataClassification::default(), DataClassification::Public);
    }

    #[test]
    fn test_data_classification_required_trust_levels() {
        assert_eq!(
            DataClassification::Public.required_trust_level(),
            TrustLevel::Untrusted
        );
        assert_eq!(
            DataClassification::Internal.required_trust_level(),
            TrustLevel::Cloud
        );
        assert_eq!(
            DataClassification::Confidential.required_trust_level(),
            TrustLevel::Local
        );
    }

    #[test]
    fn test_data_classification_as_str() {
        assert_eq!(DataClassification::Public.as_str(), "public");
        assert_eq!(DataClassification::Internal.as_str(), "internal");
        assert_eq!(DataClassification::Confidential.as_str(), "confidential");
    }

    #[test]
    fn test_data_classification_display() {
        assert_eq!(DataClassification::Public.to_string(), "public");
        assert_eq!(DataClassification::Internal.to_string(), "internal");
        assert_eq!(DataClassification::Confidential.to_string(), "confidential");
    }

    // ===== Satisfies Tests =====

    #[test]
    fn test_trust_level_satisfies_public() {
        // Public data can be accessed from any trust level
        assert!(TrustLevel::Local.satisfies(DataClassification::Public));
        assert!(TrustLevel::Cloud.satisfies(DataClassification::Public));
        assert!(TrustLevel::Untrusted.satisfies(DataClassification::Public));
    }

    #[test]
    fn test_trust_level_satisfies_internal() {
        // Internal data requires Cloud or higher
        assert!(TrustLevel::Local.satisfies(DataClassification::Internal));
        assert!(TrustLevel::Cloud.satisfies(DataClassification::Internal));
        assert!(!TrustLevel::Untrusted.satisfies(DataClassification::Internal));
    }

    #[test]
    fn test_trust_level_satisfies_confidential() {
        // Confidential data requires Local
        assert!(TrustLevel::Local.satisfies(DataClassification::Confidential));
        assert!(!TrustLevel::Cloud.satisfies(DataClassification::Confidential));
        assert!(!TrustLevel::Untrusted.satisfies(DataClassification::Confidential));
    }

    // ===== Serialization Tests =====

    #[test]
    fn test_trust_level_serde_roundtrip() {
        let variants = [TrustLevel::Local, TrustLevel::Cloud, TrustLevel::Untrusted];
        for variant in &variants {
            let json = serde_json::to_string(variant).expect("Failed to serialize");
            let deserialized: TrustLevel =
                serde_json::from_str(&json).expect("Failed to deserialize");
            assert_eq!(deserialized, *variant, "Roundtrip failed for {:?}", variant);
        }
    }

    #[test]
    fn test_data_classification_serde_roundtrip() {
        let variants = [
            DataClassification::Public,
            DataClassification::Internal,
            DataClassification::Confidential,
        ];
        for variant in &variants {
            let json = serde_json::to_string(variant).expect("Failed to serialize");
            let deserialized: DataClassification =
                serde_json::from_str(&json).expect("Failed to deserialize");
            assert_eq!(deserialized, *variant, "Roundtrip failed for {:?}", variant);
        }
    }

    #[test]
    fn test_trust_level_serde_lowercase() {
        assert_eq!(
            serde_json::to_string(&TrustLevel::Local).unwrap(),
            "\"local\""
        );
        assert_eq!(
            serde_json::to_string(&TrustLevel::Cloud).unwrap(),
            "\"cloud\""
        );
        assert_eq!(
            serde_json::to_string(&TrustLevel::Untrusted).unwrap(),
            "\"untrusted\""
        );
    }

    #[test]
    fn test_data_classification_serde_lowercase() {
        assert_eq!(
            serde_json::to_string(&DataClassification::Public).unwrap(),
            "\"public\""
        );
        assert_eq!(
            serde_json::to_string(&DataClassification::Internal).unwrap(),
            "\"internal\""
        );
        assert_eq!(
            serde_json::to_string(&DataClassification::Confidential).unwrap(),
            "\"confidential\""
        );
    }
}
