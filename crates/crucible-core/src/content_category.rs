use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Universal content categories for all link types
///
/// This enum provides a standardized way to classify different types of content
/// across the Crucible knowledge management system. It's designed to be used
/// for note classification, link processing, and content organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContentCategory {
    /// Markdown files (.md, .markdown)
    Note,
    /// Image files (.png, .jpg, .svg, .gif, .webp)
    Image,
    /// Video files (.mp4, .avi, .mov, .webm)
    Video,
    /// Audio files (.mp3, .wav, .ogg, .flac)
    Audio,
    /// PDF documents
    PDF,
    /// Other document types (.doc, .txt, .rtf, etc.)
    Document,
    /// Unrecognized file types or plaintext
    Other,
    /// General web pages (no specific platform)
    Web,
    /// YouTube videos and content
    YouTube,
    /// GitHub repositories, issues, and discussions
    GitHub,
    /// Wikipedia articles and pages
    Wikipedia,
    /// Stack Overflow questions and answers
    StackOverflow,
}

// Constants for each category to enable ergonomic usage
impl ContentCategory {
    /// Markdown files category
    pub const NOTE: Self = Self::Note;
    /// Image files category
    pub const IMAGE: Self = Self::Image;
    /// Video files category
    pub const VIDEO: Self = Self::Video;
    /// Audio files category
    pub const AUDIO: Self = Self::Audio;
    /// PDF files category
    pub const PDF: Self = Self::PDF;
    /// Note files category
    pub const DOCUMENT: Self = Self::Note;
    /// Other files category
    pub const OTHER: Self = Self::Other;
    /// Web content category
    pub const WEB: Self = Self::Web;
    /// YouTube content category
    pub const YOUTUBE: Self = Self::YouTube;
    /// GitHub content category
    pub const GITHUB: Self = Self::GitHub;
    /// Wikipedia content category
    pub const WIKIPEDIA: Self = Self::Wikipedia;
    /// Stack Overflow content category
    pub const STACK_OVERFLOW: Self = Self::StackOverflow;
}

impl ContentCategory {
    /// Convert to a lowercase string for database storage and serialization
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentCategory::Note => "note",
            ContentCategory::Image => "image",
            ContentCategory::Video => "video",
            ContentCategory::Audio => "audio",
            ContentCategory::PDF => "pdf",
            ContentCategory::Document => "document",
            ContentCategory::Other => "other",
            ContentCategory::Web => "web",
            ContentCategory::YouTube => "youtube",
            ContentCategory::GitHub => "github",
            ContentCategory::Wikipedia => "wikipedia",
            ContentCategory::StackOverflow => "stackoverflow",
        }
    }

    /// Get all available content categories
    pub fn all() -> &'static [ContentCategory] {
        &[
            ContentCategory::Note,
            ContentCategory::Image,
            ContentCategory::Video,
            ContentCategory::Audio,
            ContentCategory::PDF,
            ContentCategory::Document,
            ContentCategory::Other,
            ContentCategory::Web,
            ContentCategory::YouTube,
            ContentCategory::GitHub,
            ContentCategory::Wikipedia,
            ContentCategory::StackOverflow,
        ]
    }

    /// Check if this category represents a file (local content)
    pub fn is_file(&self) -> bool {
        matches!(
            self,
            ContentCategory::Note
                | ContentCategory::Image
                | ContentCategory::Video
                | ContentCategory::Audio
                | ContentCategory::PDF
                | ContentCategory::Document
                | ContentCategory::Other
        )
    }

    /// Check if this category represents web content
    pub fn is_web_content(&self) -> bool {
        matches!(
            self,
            ContentCategory::Web
                | ContentCategory::YouTube
                | ContentCategory::GitHub
                | ContentCategory::Wikipedia
                | ContentCategory::StackOverflow
        )
    }

    /// Get common file extensions for this category (if applicable)
    pub fn file_extensions(&self) -> &'static [&'static str] {
        match self {
            ContentCategory::Note => &["md", "markdown", "txt"],
            ContentCategory::Image => &["png", "jpg", "jpeg", "svg", "gif", "webp", "bmp", "tiff"],
            ContentCategory::Video => &["mp4", "avi", "mov", "webm", "mkv", "flv"],
            ContentCategory::Audio => &["mp3", "wav", "ogg", "flac", "aac", "m4a"],
            ContentCategory::PDF => &["pdf"],
            ContentCategory::Document => &["doc", "docx", "txt", "rtf", "odt"],
            ContentCategory::Other => &[],
            ContentCategory::Web => &["html", "htm"],
            ContentCategory::YouTube => &[],
            ContentCategory::GitHub => &[],
            ContentCategory::Wikipedia => &[],
            ContentCategory::StackOverflow => &[],
        }
    }
}

/// Implement Display for user-friendly string representation
impl fmt::Display for ContentCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let display_name = match self {
            ContentCategory::Note => "Note",
            ContentCategory::Image => "Image",
            ContentCategory::Video => "Video",
            ContentCategory::Audio => "Audio",
            ContentCategory::PDF => "PDF",
            ContentCategory::Document => "Document",
            ContentCategory::Other => "Other",
            ContentCategory::Web => "Web",
            ContentCategory::YouTube => "YouTube",
            ContentCategory::GitHub => "GitHub",
            ContentCategory::Wikipedia => "Wikipedia",
            ContentCategory::StackOverflow => "Stack Overflow",
        };
        write!(f, "{}", display_name)
    }
}

/// Implement FromStr for parsing strings into ContentCategory
impl FromStr for ContentCategory {
    type Err = ContentCategoryError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "note" => Ok(ContentCategory::Note),
            "image" => Ok(ContentCategory::Image),
            "video" => Ok(ContentCategory::Video),
            "audio" => Ok(ContentCategory::Audio),
            "pdf" => Ok(ContentCategory::PDF),
            "document" => Ok(ContentCategory::Document),
            "other" => Ok(ContentCategory::Other),
            "web" => Ok(ContentCategory::Web),
            "youtube" => Ok(ContentCategory::YouTube),
            "github" => Ok(ContentCategory::GitHub),
            "wikipedia" => Ok(ContentCategory::Wikipedia),
            "stackoverflow" => Ok(ContentCategory::StackOverflow),
            _ => Err(ContentCategoryError::UnknownCategory(s.to_string())),
        }
    }
}

/// Implement TryFrom<String> for robust conversion
impl TryFrom<String> for ContentCategory {
    type Error = ContentCategoryError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

/// Implement TryFrom<&str> for convenient conversion
impl TryFrom<&str> for ContentCategory {
    type Error = ContentCategoryError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

/// Error type for ContentCategory parsing failures
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContentCategoryError {
    #[error("Unknown content category: {0}")]
    UnknownCategory(String),
}

/// Default implementation defaults to Other
impl Default for ContentCategory {
    fn default() -> Self {
        ContentCategory::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_category_display() {
        assert_eq!(ContentCategory::Note.to_string(), "Note");
        assert_eq!(ContentCategory::YouTube.to_string(), "YouTube");
        assert_eq!(ContentCategory::StackOverflow.to_string(), "Stack Overflow");
    }

    #[test]
    fn test_content_category_from_str() {
        assert_eq!(
            ContentCategory::from_str("note").unwrap(),
            ContentCategory::Note
        );
        assert_eq!(
            ContentCategory::from_str("YouTube").unwrap(),
            ContentCategory::YouTube
        );
        assert_eq!(
            ContentCategory::from_str("STACKOVERFLOW").unwrap(),
            ContentCategory::StackOverflow
        );
    }

    #[test]
    fn test_content_category_from_str_invalid() {
        let result = ContentCategory::from_str("invalid");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ContentCategoryError::UnknownCategory(_)
        ));
    }

    #[test]
    fn test_content_category_as_str() {
        assert_eq!(ContentCategory::Note.as_str(), "note");
        assert_eq!(ContentCategory::YouTube.as_str(), "youtube");
        assert_eq!(ContentCategory::StackOverflow.as_str(), "stackoverflow");
    }

    #[test]
    fn test_is_file() {
        assert!(ContentCategory::Note.is_file());
        assert!(ContentCategory::PDF.is_file());
        assert!(!ContentCategory::Web.is_file());
        assert!(!ContentCategory::YouTube.is_file());
    }

    #[test]
    fn test_is_web_content() {
        assert!(ContentCategory::Web.is_web_content());
        assert!(ContentCategory::GitHub.is_web_content());
        assert!(!ContentCategory::Note.is_web_content());
        assert!(!ContentCategory::PDF.is_web_content());
    }

    #[test]
    fn test_file_extensions() {
        let note_exts = ContentCategory::Note.file_extensions();
        assert!(note_exts.contains(&"md"));
        assert!(note_exts.contains(&"markdown"));

        let image_exts = ContentCategory::Image.file_extensions();
        assert!(image_exts.contains(&"png"));
        assert!(image_exts.contains(&"jpg"));

        let web_exts = ContentCategory::YouTube.file_extensions();
        assert!(web_exts.is_empty());
    }

    #[test]
    fn test_constants() {
        assert_eq!(ContentCategory::NOTE, ContentCategory::Note);
        assert_eq!(ContentCategory::GITHUB, ContentCategory::GitHub);
        assert_eq!(
            ContentCategory::STACK_OVERFLOW,
            ContentCategory::StackOverflow
        );
    }

    #[test]
    fn test_all() {
        let all = ContentCategory::all();
        assert_eq!(all.len(), 12);
        assert!(all.contains(&ContentCategory::Note));
        assert!(all.contains(&ContentCategory::StackOverflow));
    }

    #[test]
    fn test_serialization_roundtrip() {
        let original = ContentCategory::YouTube;
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: ContentCategory = serde_json::from_str(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_try_from_string() {
        assert_eq!(
            ContentCategory::try_from("pdf".to_string()).unwrap(),
            ContentCategory::PDF
        );
        assert!(ContentCategory::try_from("invalid".to_string()).is_err());
    }

    #[test]
    fn test_try_from_str() {
        assert_eq!(
            ContentCategory::try_from("github").unwrap(),
            ContentCategory::GitHub
        );
        assert!(ContentCategory::try_from("invalid").is_err());
    }

    #[test]
    fn test_default() {
        assert_eq!(ContentCategory::default(), ContentCategory::Other);
    }
}
