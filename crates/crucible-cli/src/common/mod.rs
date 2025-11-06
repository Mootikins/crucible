//! Common utilities and shared components for Crucible CLI

pub mod file_scanner;
pub mod kiln_processor;
pub mod tool_manager;
pub mod change_detection_service;

pub use file_scanner::{
    FileScanningService, default_kiln_scan_config, performance_scan_config, development_scan_config
};
pub use kiln_processor::{KilnProcessor, ProcessingResult};
pub use tool_manager::{ToolRegistry, CrucibleToolManager, ToolDefinition};
pub use change_detection_service::{
    ChangeDetectionService, ChangeDetectionServiceConfig, ChangeDetectionServiceResult,
    ChangeDetectionServiceMetrics, ChangeProcessingResult,
    development_config as change_detection_development_config,
    production_config as change_detection_production_config,
    detection_only_config,
};
