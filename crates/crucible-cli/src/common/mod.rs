//! Common utilities and shared components for Crucible CLI

pub mod change_detection_service;
pub mod file_scanner;
pub mod kiln_processor;
pub mod tool_manager;

pub use change_detection_service::{
    detection_only_config, development_config as change_detection_development_config,
    production_config as change_detection_production_config, ChangeDetectionService,
    ChangeDetectionServiceConfig, ChangeDetectionServiceMetrics, ChangeDetectionServiceResult,
    ChangeProcessingResult,
};
pub use file_scanner::{
    default_kiln_scan_config, development_scan_config, performance_scan_config, FileScanningService,
};
pub use kiln_processor::{KilnProcessor, ProcessingResult};
pub use tool_manager::{CrucibleToolManager, ToolDefinition, ToolRegistry};
