//! Performance validation tests for Phase 8.4
//!
//! This module validates system performance under realistic load conditions.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use super::{
    IntegrationTestRunner, TestResult, TestCategory, TestOutcome, TestUtilities,
    PerformanceValidator, default_performance_requirements,
};

/// Performance validation tests
pub struct PerformanceValidationTests {
    test_runner: Arc<IntegrationTestRunner>,
    test_utils: Arc<TestUtilities>,
    performance_validator: Arc<PerformanceValidator>,
}

impl PerformanceValidationTests {
    pub fn new(
        test_runner: Arc<IntegrationTestRunner>,
        test_utils: Arc<TestUtilities>,
    ) -> Self {
        let requirements = default_performance_requirements();
        let performance_validator = Arc::new(PerformanceValidator::new(
            test_runner.clone(),
            requirements,
        ));

        Self {
            test_runner,
            test_utils,
            performance_validator,
        }
    }

    pub async fn run_performance_validation_tests(&self) -> Result<Vec<TestResult>> {
        info!("Starting performance validation tests");

        let results = self.performance_validator.run_performance_validation().await?;

        info!("Performance validation tests completed");
        Ok(results)
    }
}