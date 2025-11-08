//! Deduplication Reporting Module
//!
//! This module provides comprehensive reporting functionality for block deduplication
//! analysis, including formatted reports, visualizations, and export capabilities.

use crate::deduplication_detector::{DeduplicationDetector, SurrealDeduplicationDetector};
use crate::SurrealDbConfig;
use chrono::{DateTime, Utc};
use comfy_table::{Attribute, Cell, Color, Row, Table};
use crucible_core::storage::{DeduplicationStorage, StorageError, StorageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Comprehensive deduplication report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeduplicationReport {
    /// Report metadata
    pub metadata: ReportMetadata,
    /// Executive summary
    pub summary: ExecutiveSummary,
    /// Detailed statistics
    pub detailed_stats: DetailedStatistics,
    /// Block type analysis
    pub block_type_analysis: BlockTypeAnalysis,
    /// Most duplicated blocks
    pub top_duplicates: Vec<DuplicateBlockReport>,
    /// Storage efficiency metrics
    pub storage_metrics: StorageEfficiencyMetrics,
    /// Recommendations
    pub recommendations: Vec<Recommendation>,
}

/// Report metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportMetadata {
    /// Report title
    pub title: String,
    /// Generated at timestamp
    pub generated_at: DateTime<Utc>,
    /// Report version
    pub version: String,
    /// Data source
    pub data_source: String,
    /// Analysis period
    pub analysis_period: Option<AnalysisPeriod>,
}

/// Analysis period for the report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisPeriod {
    /// Start of analysis period
    pub start_date: DateTime<Utc>,
    /// End of analysis period
    pub end_date: DateTime<Utc>,
}

/// Executive summary for quick insights
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutiveSummary {
    /// Total blocks analyzed
    pub total_blocks: usize,
    /// Unique blocks
    pub unique_blocks: usize,
    /// Duplicate blocks
    pub duplicate_blocks: usize,
    /// Deduplication ratio (percentage)
    pub deduplication_ratio_percent: f64,
    /// Storage saved
    pub storage_saved_mb: f64,
    /// Storage efficiency score (0-100)
    pub storage_efficiency_score: u8,
    /// Key finding
    pub key_finding: String,
}

/// Detailed statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedStatistics {
    /// Total unique blocks
    pub total_unique_blocks: usize,
    /// Total block instances (including duplicates)
    pub total_block_instances: usize,
    /// Number of duplicate blocks
    pub duplicate_blocks: usize,
    /// Overall deduplication ratio
    pub deduplication_ratio: f64,
    /// Total storage saved by deduplication
    pub total_storage_saved: usize,
    /// Average block size
    pub average_block_size: usize,
    /// Statistics calculated at
    pub calculated_at: DateTime<Utc>,
}

/// Block type analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTypeAnalysis {
    /// Distribution of block types
    pub block_type_distribution: HashMap<String, BlockTypeStats>,
    /// Most duplicated block types
    pub most_duplicated_types: Vec<BlockTypeStats>,
}

/// Statistics for a specific block type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTypeStats {
    /// Block type name
    pub block_type: String,
    /// Total count
    pub total_count: usize,
    /// Unique count
    pub unique_count: usize,
    /// Duplicate count
    pub duplicate_count: usize,
    /// Deduplication ratio
    pub deduplication_ratio: f64,
    /// Average size
    pub average_size: usize,
    /// Storage saved
    pub storage_saved: usize,
}

/// Report entry for a duplicate block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateBlockReport {
    /// Block hash
    pub block_hash: String,
    /// Number of occurrences
    pub occurrence_count: usize,
    /// Documents containing this block
    pub documents: Vec<String>,
    /// Estimated block size
    pub estimated_block_size: usize,
    /// Storage saved by deduplication
    pub storage_saved: usize,
    /// Content preview
    pub content_preview: String,
    /// Block type
    pub block_type: String,
    /// First seen timestamp
    pub first_seen: DateTime<Utc>,
    /// Last seen timestamp
    pub last_seen: DateTime<Utc>,
}

/// Storage efficiency metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageEfficiencyMetrics {
    /// Total storage used for blocks
    pub total_block_storage_mb: f64,
    /// Storage saved by deduplication
    pub deduplication_savings_mb: f64,
    /// Number of stored blocks
    pub stored_block_count: usize,
    /// Number of unique blocks
    pub unique_block_count: usize,
    /// Average block size
    pub average_block_size: usize,
    /// Storage efficiency ratio (unique / total)
    pub storage_efficiency: f64,
    /// Potential savings with better deduplication
    pub potential_savings_mb: f64,
}

/// Recommendation for optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Recommendation title
    pub title: String,
    /// Detailed description
    pub description: String,
    /// Priority level
    pub priority: RecommendationPriority,
    /// Estimated impact
    pub estimated_impact: String,
    /// Implementation effort
    pub implementation_effort: ImplementationEffort,
}

/// Priority level for recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecommendationPriority {
    High,
    Medium,
    Low,
}

/// Implementation effort required
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImplementationEffort {
    Low,
    Medium,
    High,
}

/// Report generation options
#[derive(Debug, Clone)]
pub struct ReportOptions {
    /// Include detailed block analysis
    pub include_detailed_blocks: bool,
    /// Maximum number of top duplicates to include
    pub max_top_duplicates: usize,
    /// Include recommendations
    pub include_recommendations: bool,
    /// Analysis period (optional)
    pub analysis_period: Option<AnalysisPeriod>,
    /// Export format
    pub export_format: ExportFormat,
}

/// Export format options
#[derive(Debug, Clone)]
pub enum ExportFormat {
    Text,
    Json,
    Csv,
    Markdown,
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            include_detailed_blocks: true,
            max_top_duplicates: 20,
            include_recommendations: true,
            analysis_period: None,
            export_format: ExportFormat::Text,
        }
    }
}

/// Deduplication report generator
/// Generic deduplication report generator
///
/// This struct generates comprehensive deduplication reports for any storage
/// backend that implements the `DeduplicationStorage` trait.
///
/// # Generic Parameters
///
/// * `S` - The storage backend type (implements `DeduplicationStorage`)
pub struct DeduplicationReportGenerator<S: DeduplicationStorage> {
    detector: DeduplicationDetector<S>,
}

impl<S: DeduplicationStorage> DeduplicationReportGenerator<S> {
    /// Create a new report generator from a detector
    ///
    /// # Arguments
    ///
    /// * `detector` - The deduplication detector to use for report generation
    pub fn from_detector(detector: DeduplicationDetector<S>) -> Self {
        Self { detector }
    }

    /// Generate a comprehensive deduplication report
    ///
    /// # Arguments
    ///
    /// * `options` - Report generation options
    ///
    /// # Returns
    ///
    /// A complete deduplication report with statistics, analysis, and recommendations
    pub async fn generate_report(
        &self,
        options: ReportOptions,
    ) -> StorageResult<DeduplicationReport> {
        // Get comprehensive deduplication statistics
        let stats = self.detector.get_all_deduplication_stats().await?;
        let storage_usage = self.detector.get_storage_usage_stats().await?;

        // Get top duplicate blocks
        let top_duplicates = self.detector.find_duplicate_blocks(2).await?;
        let mut duplicate_reports = Vec::new();

        for duplicate in top_duplicates.into_iter().take(options.max_top_duplicates) {
            duplicate_reports.push(DuplicateBlockReport {
                block_hash: duplicate.block_hash,
                occurrence_count: duplicate.occurrence_count,
                documents: duplicate.documents,
                estimated_block_size: duplicate.estimated_block_size,
                storage_saved: duplicate.storage_saved,
                content_preview: duplicate.content_preview,
                block_type: duplicate.block_type,
                first_seen: duplicate.first_seen,
                last_seen: duplicate.last_seen,
            });
        }

        // Calculate executive summary metrics
        let storage_saved_mb = stats.total_storage_saved as f64 / (1024.0 * 1024.0);
        let total_block_storage_mb = storage_usage.total_block_storage as f64 / (1024.0 * 1024.0);
        let deduplication_ratio_percent = stats.deduplication_ratio * 100.0;
        let storage_efficiency_score = (storage_usage.storage_efficiency * 100.0) as u8;

        // Generate key finding
        let key_finding = if stats.deduplication_ratio > 0.5 {
            format!("High deduplication ratio ({:.1}%) indicates significant content reuse across documents", deduplication_ratio_percent)
        } else if stats.deduplication_ratio > 0.2 {
            format!(
                "Moderate deduplication ratio ({:.1}%) shows some content reuse patterns",
                deduplication_ratio_percent
            )
        } else {
            format!(
                "Low deduplication ratio ({:.1}%) suggests mostly unique content across documents",
                deduplication_ratio_percent
            )
        };

        // Generate recommendations
        let recommendations = self.generate_recommendations(&stats, &storage_usage).await;

        Ok(DeduplicationReport {
            metadata: ReportMetadata {
                title: "Block Deduplication Analysis Report".to_string(),
                generated_at: Utc::now(),
                version: "1.0.0".to_string(),
                data_source: "SurrealDB Content-Addressed Storage".to_string(),
                analysis_period: options.analysis_period,
            },
            summary: ExecutiveSummary {
                total_blocks: stats.total_block_instances,
                unique_blocks: stats.total_unique_blocks,
                duplicate_blocks: stats.duplicate_blocks,
                deduplication_ratio_percent,
                storage_saved_mb,
                storage_efficiency_score,
                key_finding,
            },
            detailed_stats: DetailedStatistics {
                total_unique_blocks: stats.total_unique_blocks,
                total_block_instances: stats.total_block_instances,
                duplicate_blocks: stats.duplicate_blocks,
                deduplication_ratio: stats.deduplication_ratio,
                total_storage_saved: stats.total_storage_saved,
                average_block_size: stats.average_block_size,
                calculated_at: stats.calculated_at,
            },
            block_type_analysis: BlockTypeAnalysis {
                block_type_distribution: HashMap::new(), // Would need additional analysis
                most_duplicated_types: Vec::new(),
            },
            top_duplicates: duplicate_reports,
            storage_metrics: StorageEfficiencyMetrics {
                total_block_storage_mb,
                deduplication_savings_mb: storage_saved_mb,
                stored_block_count: storage_usage.stored_block_count,
                unique_block_count: storage_usage.unique_block_count,
                average_block_size: storage_usage.average_block_size,
                storage_efficiency: storage_usage.storage_efficiency,
                potential_savings_mb: 0.0, // Would need additional analysis
            },
            recommendations,
        })
    }

    /// Generate optimization recommendations
    async fn generate_recommendations(
        &self,
        stats: &crucible_core::storage::deduplication_traits::DeduplicationStats,
        storage_usage: &crucible_core::storage::deduplication_traits::StorageUsageStats,
    ) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        // High deduplication ratio recommendation
        if stats.deduplication_ratio > 0.5 {
            recommendations.push(Recommendation {
                title: "Consider template extraction for highly duplicated content".to_string(),
                description: "Your system shows high content duplication. Consider extracting common patterns into templates or reusable components to further optimize storage.".to_string(),
                priority: RecommendationPriority::High,
                estimated_impact: "High - Could reduce storage by 20-40%".to_string(),
                implementation_effort: ImplementationEffort::Medium,
            });
        }

        // Low efficiency recommendation
        if storage_usage.storage_efficiency < 0.7 {
            recommendations.push(Recommendation {
                title: "Implement block-level change detection".to_string(),
                description: "Low storage efficiency indicates opportunities for better change detection. Implement block-level analysis to identify and optimize duplicate content.".to_string(),
                priority: RecommendationPriority::Medium,
                estimated_impact: "Medium - Could improve efficiency by 15-25%".to_string(),
                implementation_effort: ImplementationEffort::High,
            });
        }

        // Large block size recommendation
        if stats.average_block_size > 500 {
            recommendations.push(Recommendation {
                title: "Optimize large blocks for better deduplication".to_string(),
                description: "Large average block size may be limiting deduplication effectiveness. Consider breaking down large blocks into smaller, more reusable components.".to_string(),
                priority: RecommendationPriority::Medium,
                estimated_impact: "Medium - Could increase deduplication ratio".to_string(),
                implementation_effort: ImplementationEffort::Medium,
            });
        }

        // General optimization recommendation
        if recommendations.is_empty() {
            recommendations.push(Recommendation {
                title: "Continue monitoring deduplication metrics".to_string(),
                description: "Your current deduplication setup appears effective. Continue monitoring metrics to identify optimization opportunities as your content base grows.".to_string(),
                priority: RecommendationPriority::Low,
                estimated_impact: "Low - Maintenance and monitoring".to_string(),
                implementation_effort: ImplementationEffort::Low,
            });
        }

        recommendations
    }

    /// Export report to specified format
    pub fn export_report(
        &self,
        report: &DeduplicationReport,
        format: &ExportFormat,
    ) -> StorageResult<String> {
        match format {
            ExportFormat::Text => Ok(self.format_as_text(report)),
            ExportFormat::Json => serde_json::to_string_pretty(report)
                .map_err(|e| StorageError::backend(format!("Failed to serialize report: {}", e))),
            ExportFormat::Markdown => Ok(self.format_as_markdown(report)),
            ExportFormat::Csv => Ok(self.format_as_csv(report)),
        }
    }

    /// Format report as text table
    fn format_as_text(&self, report: &DeduplicationReport) -> String {
        let mut output = String::new();

        // Header
        output.push_str(&format!("{}\n", &report.metadata.title));
        output.push_str(&format!(
            "Generated: {}\n\n",
            report.metadata.generated_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));

        // Executive Summary Table
        let mut summary_table = Table::new();
        summary_table.set_header(vec!["Metric", "Value"]);

        summary_table.add_row(Row::from(vec![
            Cell::new("Total Blocks").add_attribute(Attribute::Bold),
            Cell::new(&report.summary.total_blocks.to_string()),
        ]));

        summary_table.add_row(Row::from(vec![
            Cell::new("Unique Blocks").add_attribute(Attribute::Bold),
            Cell::new(&report.summary.unique_blocks.to_string()),
        ]));

        summary_table.add_row(Row::from(vec![
            Cell::new("Duplicate Blocks").add_attribute(Attribute::Bold),
            Cell::new(&report.summary.duplicate_blocks.to_string()),
        ]));

        summary_table.add_row(Row::from(vec![
            Cell::new("Deduplication Ratio").add_attribute(Attribute::Bold),
            Cell::new(&format!(
                "{:.1}%",
                report.summary.deduplication_ratio_percent
            ))
            .add_attribute(Attribute::Bold)
            .fg(if report.summary.deduplication_ratio_percent > 30.0 {
                Color::Green
            } else {
                Color::Yellow
            }),
        ]));

        summary_table.add_row(Row::from(vec![
            Cell::new("Storage Saved").add_attribute(Attribute::Bold),
            Cell::new(&format!("{:.2} MB", report.summary.storage_saved_mb)),
        ]));

        summary_table.add_row(Row::from(vec![
            Cell::new("Efficiency Score").add_attribute(Attribute::Bold),
            Cell::new(&format!("{}/100", report.summary.storage_efficiency_score))
                .add_attribute(Attribute::Bold)
                .fg(if report.summary.storage_efficiency_score > 70 {
                    Color::Green
                } else {
                    Color::Yellow
                }),
        ]));

        output.push_str("Executive Summary:\n");
        output.push_str(&summary_table.to_string());
        output.push('\n');

        // Key Finding
        output.push_str(&format!(
            "\nKey Finding:\n{}\n\n",
            report.summary.key_finding
        ));

        // Top Duplicates (if any)
        if !report.top_duplicates.is_empty() {
            let mut dup_table = Table::new();
            dup_table.set_header(vec!["Occurrences", "Type", "Size", "Saved", "Preview"]);

            for duplicate in &report.top_duplicates {
                dup_table.add_row(Row::from(vec![
                    Cell::new(&duplicate.occurrence_count.to_string()),
                    Cell::new(&duplicate.block_type),
                    Cell::new(&format!("{} bytes", duplicate.estimated_block_size)),
                    Cell::new(&format!("{} bytes", duplicate.storage_saved)),
                    Cell::new(&duplicate.content_preview),
                ]));
            }

            output.push_str("Top Duplicated Blocks:\n");
            output.push_str(&dup_table.to_string());
            output.push('\n');
        }

        // Recommendations
        if !report.recommendations.is_empty() {
            output.push_str("Recommendations:\n");
            for (i, rec) in report.recommendations.iter().enumerate() {
                let priority_color = match rec.priority {
                    RecommendationPriority::High => Color::Red,
                    RecommendationPriority::Medium => Color::Yellow,
                    RecommendationPriority::Low => Color::Green,
                };

                let priority_text = format!("{:?}", rec.priority);
                output.push_str(&format!("{}. {} ({})\n", i + 1, rec.title, priority_text));
                output.push_str(&format!("   {}\n", rec.description));
                output.push_str(&format!("   Impact: {}\n", rec.estimated_impact));
                output.push_str(&format!("   Effort: {:?}\n\n", rec.implementation_effort));
            }
        }

        output
    }

    /// Format report as Markdown
    fn format_as_markdown(&self, report: &DeduplicationReport) -> String {
        let mut output = String::new();

        // Title and metadata
        output.push_str(&format!("# {}\n\n", report.metadata.title));
        output.push_str(&format!(
            "**Generated:** {}  \n",
            report.metadata.generated_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));
        output.push_str(&format!("**Version:** {}  \n", report.metadata.version));
        output.push_str(&format!(
            "**Data Source:** {}\n\n",
            report.metadata.data_source
        ));

        // Executive Summary
        output.push_str("## Executive Summary\n\n");
        output.push_str("| Metric | Value |\n");
        output.push_str("|--------|-------|\n");
        output.push_str(&format!(
            "| Total Blocks | {} |\n",
            report.summary.total_blocks
        ));
        output.push_str(&format!(
            "| Unique Blocks | {} |\n",
            report.summary.unique_blocks
        ));
        output.push_str(&format!(
            "| Duplicate Blocks | {} |\n",
            report.summary.duplicate_blocks
        ));
        output.push_str(&format!(
            "| Deduplication Ratio | {:.1}% |\n",
            report.summary.deduplication_ratio_percent
        ));
        output.push_str(&format!(
            "| Storage Saved | {:.2} MB |\n",
            report.summary.storage_saved_mb
        ));
        output.push_str(&format!(
            "| Efficiency Score | {}/100 |\n\n",
            report.summary.storage_efficiency_score
        ));

        // Key Finding
        output.push_str(&format!(
            "### Key Finding\n\n{}\n\n",
            report.summary.key_finding
        ));

        // Top Duplicates
        if !report.top_duplicates.is_empty() {
            output.push_str("## Top Duplicated Blocks\n\n");
            output.push_str("| Occurrences | Type | Size | Saved | Preview |\n");
            output.push_str("|-------------|------|------|-------|---------|\n");

            for duplicate in &report.top_duplicates {
                output.push_str(&format!(
                    "| {} | {} | {} bytes | {} bytes | {} |\n",
                    duplicate.occurrence_count,
                    duplicate.block_type,
                    duplicate.estimated_block_size,
                    duplicate.storage_saved,
                    duplicate.content_preview
                ));
            }
            output.push('\n');
        }

        // Recommendations
        if !report.recommendations.is_empty() {
            output.push_str("## Recommendations\n\n");

            for (i, rec) in report.recommendations.iter().enumerate() {
                output.push_str(&format!("### {}. {}\n\n", i + 1, rec.title));
                output.push_str(&format!("**Priority:** {:?}\n\n", rec.priority));
                output.push_str(&format!("**Description:** {}\n\n", rec.description));
                output.push_str(&format!(
                    "**Estimated Impact:** {}\n\n",
                    rec.estimated_impact
                ));
                output.push_str(&format!(
                    "**Implementation Effort:** {:?}\n\n",
                    rec.implementation_effort
                ));
            }
        }

        output
    }

    /// Format report as CSV
    fn format_as_csv(&self, report: &DeduplicationReport) -> String {
        let mut output = String::new();

        // Summary section
        output.push_str("Section,Metric,Value\n");
        output.push_str(&format!(
            "Summary,Total Blocks,{}\n",
            report.summary.total_blocks
        ));
        output.push_str(&format!(
            "Summary,Unique Blocks,{}\n",
            report.summary.unique_blocks
        ));
        output.push_str(&format!(
            "Summary,Duplicate Blocks,{}\n",
            report.summary.duplicate_blocks
        ));
        output.push_str(&format!(
            "Summary,Deduplication Ratio,{:.1}%\n",
            report.summary.deduplication_ratio_percent
        ));
        output.push_str(&format!(
            "Summary,Storage Saved MB,{:.2}\n",
            report.summary.storage_saved_mb
        ));
        output.push_str(&format!(
            "Summary,Efficiency Score,{}\n",
            report.summary.storage_efficiency_score
        ));

        // Top duplicates
        output.push_str("\nBlockHash,Occurrences,Type,SizeBytes,StorageSaved,Preview\n");
        for duplicate in &report.top_duplicates {
            output.push_str(&format!(
                "{},{},{},{},{},{}\n",
                duplicate.block_hash,
                duplicate.occurrence_count,
                duplicate.block_type,
                duplicate.estimated_block_size,
                duplicate.storage_saved,
                duplicate
                    .content_preview
                    .replace(',', ";")
                    .replace('\n', " ")
            ));
        }

        output
    }
}

// ==================== SURREALDB-SPECIFIC IMPLEMENTATION ====================

/// SurrealDB-specific implementation for convenience constructors
impl
    DeduplicationReportGenerator<crate::content_addressed_storage::ContentAddressedStorageSurrealDB>
{
    /// Create a new report generator with SurrealDB backend
    ///
    /// This is a convenience constructor for the common SurrealDB use case.
    ///
    /// # Arguments
    ///
    /// * `config` - SurrealDB configuration
    ///
    /// # Returns
    ///
    /// A report generator ready to use with SurrealDB storage
    pub async fn new(config: SurrealDbConfig) -> StorageResult<Self> {
        let storage =
            crate::content_addressed_storage::ContentAddressedStorageSurrealDB::new(config).await?;
        let detector = DeduplicationDetector::new(storage);
        Ok(Self { detector })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_options_default() {
        let options = ReportOptions::default();
        assert!(options.include_detailed_blocks);
        assert_eq!(options.max_top_duplicates, 20);
        assert!(options.include_recommendations);
        assert!(options.analysis_period.is_none());
        assert!(matches!(options.export_format, ExportFormat::Text));
    }

    #[tokio::test]
    async fn test_format_as_text() {
        let config = SurrealDbConfig::memory();
        let storage =
            crate::content_addressed_storage::ContentAddressedStorageSurrealDB::new(config)
                .await
                .expect("Failed to create storage");
        let detector = DeduplicationDetector::new(storage);
        let generator = DeduplicationReportGenerator { detector };

        let report = DeduplicationReport {
            metadata: ReportMetadata {
                title: "Test Report".to_string(),
                generated_at: Utc::now(),
                version: "1.0.0".to_string(),
                data_source: "Test".to_string(),
                analysis_period: None,
            },
            summary: ExecutiveSummary {
                total_blocks: 100,
                unique_blocks: 80,
                duplicate_blocks: 20,
                deduplication_ratio_percent: 20.0,
                storage_saved_mb: 1.5,
                storage_efficiency_score: 80,
                key_finding: "Test finding".to_string(),
            },
            detailed_stats: DetailedStatistics {
                total_unique_blocks: 80,
                total_block_instances: 100,
                duplicate_blocks: 20,
                deduplication_ratio: 0.2,
                total_storage_saved: 1500,
                average_block_size: 200,
                calculated_at: Utc::now(),
            },
            block_type_analysis: BlockTypeAnalysis {
                block_type_distribution: HashMap::new(),
                most_duplicated_types: Vec::new(),
            },
            top_duplicates: Vec::new(),
            storage_metrics: StorageEfficiencyMetrics {
                total_block_storage_mb: 2.0,
                deduplication_savings_mb: 1.5,
                stored_block_count: 100,
                unique_block_count: 80,
                average_block_size: 200,
                storage_efficiency: 0.8,
                potential_savings_mb: 0.0,
            },
            recommendations: vec![Recommendation {
                title: "Test Recommendation".to_string(),
                description: "Test description".to_string(),
                priority: RecommendationPriority::Medium,
                estimated_impact: "Medium".to_string(),
                implementation_effort: ImplementationEffort::Low,
            }],
        };

        let text_output = generator.format_as_text(&report);
        assert!(text_output.contains("Test Report"));
        assert!(text_output.contains("100"));
        assert!(text_output.contains("20.0%"));
        assert!(text_output.contains("Test Recommendation"));
    }
}
