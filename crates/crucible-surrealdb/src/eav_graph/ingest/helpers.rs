//! Helper functions for note ingestion
//!
//! Standalone utility functions used during note ingestion.

use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};
use crucible_core::content_category::ContentCategory;
use crucible_core::parser::ParsedNote;
use std::fs;

/// Sanitize content for SurrealDB storage by removing null bytes.
///
/// SurrealDB's serialization layer (surrealdb-core) panics when strings
/// contain null bytes (0x00). This can happen in files with:
/// - ASCII art exported from drawing tools
/// - Binary data accidentally pasted into markdown
/// - Copy-pasted content with hidden control characters
///
/// This function strips null bytes to allow such files to be stored safely.
#[allow(dead_code)] // May be used for future FTS indexing
pub(crate) fn sanitize_content(s: &str) -> String {
    s.replace('\0', "")
}

/// Classify content type for universal link processing
pub(crate) fn classify_content(target: &str) -> ContentCategory {
    // Helper function to check if string is a URL
    fn is_url(s: &str) -> bool {
        s.starts_with("http://") || s.starts_with("https://")
    }

    // Helper function to extract file extension
    fn get_extension(s: &str) -> Option<&str> {
        s.rfind('.').and_then(|i| s.get(i + 1..))
    }

    // Local files - check extension
    if !is_url(target) {
        return match get_extension(target) {
            Some("md") | Some("markdown") => ContentCategory::Note,
            Some("png") | Some("jpg") | Some("jpeg") | Some("svg") | Some("gif") | Some("webp") => {
                ContentCategory::Image
            }
            Some("mp4") | Some("avi") | Some("mov") | Some("webm") | Some("mkv") => {
                ContentCategory::Video
            }
            Some("mp3") | Some("wav") | Some("ogg") | Some("flac") | Some("aac") => {
                ContentCategory::Audio
            }
            Some("pdf") => ContentCategory::PDF,
            Some("doc") | Some("docx") | Some("txt") | Some("rtf") => ContentCategory::Document,
            _ => ContentCategory::Other, // unrecognized file types
        };
    }

    // URLs - platform detection first, then general
    let target_lower = target.to_lowercase();
    if target_lower.contains("youtube.com") || target_lower.contains("youtu.be") {
        ContentCategory::YouTube
    } else if target_lower.contains("github.com") {
        ContentCategory::GitHub
    } else if target_lower.contains("wikipedia.org") {
        ContentCategory::Wikipedia
    } else if target_lower.contains("stackoverflow.com") {
        ContentCategory::StackOverflow
    } else if get_extension(target).is_some() {
        // URLs with file extensions - classify by type
        match get_extension(target) {
            Some("png") | Some("jpg") | Some("jpeg") | Some("svg") | Some("gif") => {
                ContentCategory::Image
            }
            Some("mp4") | Some("avi") | Some("mov") | Some("webm") => ContentCategory::Video,
            Some("mp3") | Some("wav") | Some("ogg") => ContentCategory::Audio,
            Some("pdf") => ContentCategory::PDF,
            _ => ContentCategory::Other,
        }
    } else {
        ContentCategory::Web // General web pages
    }
}

/// Extract timestamps from frontmatter with fallback to filesystem metadata.
///
/// Priority for created timestamp (aligns with Obsidian community conventions):
/// 1. `created` (most common in Obsidian community)
/// 2. `date-created` (alternate convention)
/// 3. `created_at` (programmatic sources fallback)
/// 4. Filesystem modified time (creation time is unreliable across platforms)
/// 5. Current time as last resort
///
/// Priority for updated timestamp:
/// 1. `modified` (most common in Obsidian community)
/// 2. `updated` (alternate convention)
/// 3. `date-modified` (alternate convention)
/// 4. `updated_at` (programmatic sources fallback)
/// 5. Filesystem modified time
/// 6. Current time as last resort
///
/// Supports both date (YYYY-MM-DD) and datetime (RFC 3339) formats.
pub(crate) fn extract_timestamps(doc: &ParsedNote) -> (DateTime<Utc>, DateTime<Utc>) {
    let now = Utc::now();

    // Helper to convert NaiveDate to DateTime<Utc> at midnight
    fn date_to_datetime(date: NaiveDate) -> DateTime<Utc> {
        // SAFETY: 0:0:0 is always a valid time - this is effectively a compile-time constant
        let midnight = NaiveTime::from_hms_opt(0, 0, 0).expect("midnight is always valid");
        let datetime = date.and_time(midnight);
        Utc.from_utc_datetime(&datetime)
    }

    // Helper to parse RFC 3339 datetime from frontmatter string
    fn parse_datetime_str(
        fm: &crucible_core::parser::Frontmatter,
        key: &str,
    ) -> Option<DateTime<Utc>> {
        let value = fm.properties().get(key)?;
        let datetime_str = value.as_str()?;
        DateTime::parse_from_rfc3339(datetime_str)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    // Helper to get datetime from frontmatter - tries datetime string first, then date
    fn get_timestamp(
        fm: &crucible_core::parser::Frontmatter,
        keys: &[&str],
    ) -> Option<DateTime<Utc>> {
        for key in keys {
            // Try RFC 3339 datetime first
            if let Some(dt) = parse_datetime_str(fm, key) {
                return Some(dt);
            }
            // Try date format (YYYY-MM-DD)
            if let Some(date) = fm.get_date(key) {
                return Some(date_to_datetime(date));
            }
        }
        None
    }

    // Try to get filesystem modified time
    let fs_mtime = fs::metadata(&doc.path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(DateTime::<Utc>::from);

    // Extract created timestamp with priority: created > date-created > created_at > fs_mtime > now
    let created_at = doc
        .frontmatter
        .as_ref()
        .and_then(|fm| get_timestamp(fm, &["created", "date-created", "created_at"]))
        .or(fs_mtime)
        .unwrap_or(now);

    // Extract updated timestamp with priority: modified > updated > date-modified > updated_at > fs_mtime > now
    let updated_at = doc
        .frontmatter
        .as_ref()
        .and_then(|fm| get_timestamp(fm, &["modified", "updated", "date-modified", "updated_at"]))
        .or(fs_mtime)
        .unwrap_or(now);

    (created_at, updated_at)
}
