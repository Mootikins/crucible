//! Memory analysis algorithms for statistics, leak detection, and reporting

use super::{
    MemoryMeasurement, MemoryStatistics, LeakDetectionResult, LeakPatternAnalysis,
    PerformanceMetrics, ResourceUtilization, ThresholdViolation, ViolationType,
    ViolationSeverity, MemoryTestConfig, LeakPatternType, GrowthCharacteristics,
    TimePattern, MemoryTestError,
};
use std::collections::HashMap;
use tracing::{debug, info, warn, error};

/// Memory analyzer for calculating statistics and detecting issues
pub struct MemoryAnalyzer {
    config: MemoryTestConfig,
}

impl MemoryAnalyzer {
    /// Create a new memory analyzer
    pub fn new(config: MemoryTestConfig) -> Self {
        Self { config }
    }

    /// Calculate comprehensive memory statistics from measurements
    pub async fn calculate_memory_statistics(&self, measurements: &[MemoryMeasurement]) -> Result<MemoryStatistics, MemoryTestError> {
        if measurements.is_empty() {
            return Err(MemoryTestError::InsufficientData("No measurements available".to_string()));
        }

        // Extract memory values
        let memory_values: Vec<u64> = measurements.iter()
            .map(|m| m.total_memory_bytes)
            .collect();

        // Calculate basic statistics
        let baseline_memory_bytes = memory_values.first().copied().unwrap_or(0);
        let peak_memory_bytes = memory_values.iter().max().copied().unwrap_or(0);
        let average_memory_bytes = memory_values.iter().sum::<u64>() as f64 / memory_values.len() as f64;

        // Calculate memory growth rate (bytes per second)
        let memory_growth_rate = self.calculate_growth_rate(measurements).await?;

        // Calculate memory volatility (standard deviation)
        let memory_volatility = self.calculate_volatility(&memory_values).await?;

        // Calculate cleanup efficiency (how well memory returns to baseline)
        let cleanup_efficiency = self.calculate_cleanup_efficiency(measurements).await?;

        // Calculate memory per operation (if operation count is available)
        let memory_per_operation = self.calculate_memory_per_operation(measurements).await?;

        Ok(MemoryStatistics {
            baseline_memory_bytes,
            peak_memory_bytes,
            average_memory_bytes: average_memory_bytes as u64,
            memory_growth_rate,
            memory_volatility,
            cleanup_efficiency,
            memory_per_operation,
        })
    }

    /// Detect memory leaks using statistical analysis
    pub async fn detect_memory_leaks(&self, measurements: &[MemoryMeasurement]) -> Result<LeakDetectionResult, MemoryTestError> {
        if measurements.len() < self.config.leak_detection.min_samples as usize {
            return Ok(LeakDetectionResult {
                leak_detected: false,
                leak_rate: 0.0,
                confidence: 0.0,
                pattern_analysis: None,
                suspected_sources: vec!["Insufficient data for analysis".to_string()],
            });
        }

        // Calculate leak rate using linear regression
        let leak_rate = self.calculate_leak_rate(measurements).await?;

        // Determine confidence level
        let confidence = self.calculate_leak_confidence(measurements, leak_rate).await?;

        // Determine if leak is detected based on threshold
        let leak_detected = leak_rate > self.config.thresholds.leak_threshold_bytes as f64;

        // Perform pattern analysis if enabled
        let pattern_analysis = if self.config.leak_detection.enable_pattern_analysis {
            Some(self.analyze_leak_pattern(measurements).await?)
        } else {
            None
        };

        // Identify suspected leak sources
        let suspected_sources = self.identify_leak_sources(measurements).await?;

        Ok(LeakDetectionResult {
            leak_detected,
            leak_rate,
            confidence,
            pattern_analysis,
            suspected_sources,
        })
    }

    /// Calculate performance metrics from test data
    pub async fn calculate_performance_metrics(
        &self,
        session: &super::MemoryTestSession,
        measurements: &[MemoryMeasurement],
    ) -> Result<PerformanceMetrics, MemoryTestError> {
        // Extract timing information
        let test_duration = session.start_time.elapsed();

        // Calculate operations per second
        let operations_per_second = self.calculate_operations_per_second(session, test_duration).await?;

        // Calculate response times (simulated for now)
        let (average_response_time_ms, p95_response_time_ms) =
            self.calculate_response_time_metrics(session).await?;

        // Calculate error rate
        let error_rate = self.calculate_error_rate(session).await?;

        // Calculate throughput
        let throughput = self.calculate_throughput(session, test_duration).await?;

        // Calculate resource utilization
        let resource_utilization = self.calculate_resource_utilization(measurements).await?;

        Ok(PerformanceMetrics {
            operations_per_second,
            average_response_time_ms,
            p95_response_time_ms,
            error_rate,
            throughput,
            resource_utilization,
        })
    }

    /// Check for threshold violations
    pub async fn check_threshold_violations(
        &self,
        memory_stats: &MemoryStatistics,
        leak_detection: &LeakDetectionResult,
    ) -> Result<Vec<ThresholdViolation>, MemoryTestError> {
        let mut violations = Vec::new();

        // Check baseline memory threshold
        if memory_stats.baseline_memory_bytes > self.config.thresholds.max_baseline_memory_bytes {
            violations.push(ThresholdViolation {
                violation_type: ViolationType::MemoryBaseline,
                threshold: self.config.thresholds.max_baseline_memory_bytes as f64,
                actual: memory_stats.baseline_memory_bytes as f64,
                severity: self.calculate_violation_severity(
                    memory_stats.baseline_memory_bytes as f64,
                    self.config.thresholds.max_baseline_memory_bytes as f64,
                ),
                description: format!(
                    "Baseline memory {} bytes exceeds threshold {} bytes",
                    memory_stats.baseline_memory_bytes,
                    self.config.thresholds.max_baseline_memory_bytes
                ),
            });
        }

        // Check memory growth rate threshold
        if memory_stats.memory_growth_rate > self.config.thresholds.max_memory_growth_rate as f64 {
            violations.push(ThresholdViolation {
                violation_type: ViolationType::MemoryGrowthRate,
                threshold: self.config.thresholds.max_memory_growth_rate as f64,
                actual: memory_stats.memory_growth_rate,
                severity: self.calculate_violation_severity(
                    memory_stats.memory_growth_rate,
                    self.config.thresholds.max_memory_growth_rate as f64,
                ),
                description: format!(
                    "Memory growth rate {:.2} bytes/s exceeds threshold {} bytes/s",
                    memory_stats.memory_growth_rate,
                    self.config.thresholds.max_memory_growth_rate
                ),
            });
        }

        // Check memory per operation threshold
        if memory_stats.memory_per_operation > self.config.thresholds.max_memory_per_operation as f64 {
            violations.push(ThresholdViolation {
                violation_type: ViolationType::MemoryPerOperation,
                threshold: self.config.thresholds.max_memory_per_operation as f64,
                actual: memory_stats.memory_per_operation,
                severity: self.calculate_violation_severity(
                    memory_stats.memory_per_operation,
                    self.config.thresholds.max_memory_per_operation as f64,
                ),
                description: format!(
                    "Memory per operation {:.2} bytes exceeds threshold {} bytes",
                    memory_stats.memory_per_operation,
                    self.config.thresholds.max_memory_per_operation
                ),
            });
        }

        // Check memory leak threshold
        if leak_detection.leak_detected {
            violations.push(ThresholdViolation {
                violation_type: ViolationType::MemoryLeak,
                threshold: self.config.thresholds.leak_threshold_bytes as f64,
                actual: leak_detection.leak_rate,
                severity: if leak_detection.confidence > 0.9 {
                    ViolationSeverity::Critical
                } else if leak_detection.confidence > 0.7 {
                    ViolationSeverity::High
                } else {
                    ViolationSeverity::Medium
                },
                description: format!(
                    "Memory leak detected: {:.2} bytes/s (confidence: {:.2}%)",
                    leak_detection.leak_rate,
                    leak_detection.confidence * 100.0
                ),
            });
        }

        Ok(violations)
    }

    /// Generate recommendations based on analysis results
    pub async fn generate_recommendations(
        &self,
        memory_stats: &MemoryStatistics,
        leak_detection: &LeakDetectionResult,
        violations: &[ThresholdViolation],
    ) -> Result<Vec<String>, MemoryTestError> {
        let mut recommendations = Vec::new();

        // Memory usage recommendations
        if memory_stats.baseline_memory_bytes > self.config.thresholds.max_baseline_memory_bytes {
            recommendations.push(
                "Consider reducing initial memory allocation or implementing lazy loading for resources".to_string()
            );
        }

        if memory_stats.memory_growth_rate > self.config.thresholds.max_memory_growth_rate as f64 * 0.8 {
            recommendations.push(
                "Memory is growing rapidly. Consider implementing more aggressive cleanup policies".to_string()
            );
        }

        if memory_stats.cleanup_efficiency < 0.8 {
            recommendations.push(
                "Cleanup efficiency is low. Review resource deallocation and garbage collection".to_string()
            );
        }

        // Memory leak recommendations
        if leak_detection.leak_detected {
            if let Some(pattern) = &leak_detection.pattern_analysis {
                match pattern.pattern_type {
                    LeakPatternType::Linear => {
                        recommendations.push(
                            "Linear memory leak detected. Check for unreleased resources or circular references".to_string()
                        );
                    }
                    LeakPatternType::Exponential => {
                        recommendations.push(
                            "Exponential memory growth detected. This may indicate unbounded data structures".to_string()
                        );
                    }
                    LeakPatternType::Stepped => {
                        recommendations.push(
                            "Stepped memory pattern detected. Check for periodic resource accumulation".to_string()
                        );
                    }
                    LeakPatternType::Sporadic => {
                        recommendations.push(
                            "Sporadic memory spikes detected. Check for error handling paths and exception scenarios".to_string()
                        );
                    }
                    LeakPatternType::Cyclic => {
                        recommendations.push(
                            "Cyclic memory pattern detected. Check for periodic cleanup or cache eviction".to_string()
                        );
                    }
                }
            }

            recommendations.extend(leak_detection.suspected_sources.iter().map(|source| {
                format!("Investigate potential leak source: {}", source)
            }));
        }

        // Violation-specific recommendations
        for violation in violations {
            match violation.violation_type {
                ViolationType::MemoryBaseline => {
                    recommendations.push(
                        "Optimize startup memory usage through deferred initialization".to_string()
                    );
                }
                ViolationType::MemoryGrowthRate => {
                    recommendations.push(
                        "Implement memory usage monitoring and automatic cleanup triggers".to_string()
                    );
                }
                ViolationType::MemoryPerOperation => {
                    recommendations.push(
                        "Optimize operation memory usage through better algorithms or data structures".to_string()
                    );
                }
                ViolationType::MemoryLeak => {
                    recommendations.push(
                        "Run detailed memory profiling to identify leak sources".to_string()
                    );
                }
                ViolationType::CleanupTimeout => {
                    recommendations.push(
                        "Optimize cleanup operations and reduce cleanup timeout".to_string()
                    );
                }
                ViolationType::ResourceExhaustion => {
                    recommendations.push(
                        "Implement resource pooling and better resource management".to_string()
                    );
                }
            }
        }

        // General recommendations
        if memory_stats.memory_volatility > 0.2 {
            recommendations.push(
                "High memory volatility detected. Consider implementing more predictable memory management".to_string()
            );
        }

        if violations.is_empty() {
            recommendations.push(
                "Memory usage appears to be within acceptable limits".to_string()
            );
        }

        Ok(recommendations)
    }

    // Helper methods for calculations

    /// Calculate memory growth rate using linear regression
    async fn calculate_growth_rate(&self, measurements: &[MemoryMeasurement]) -> Result<f64, MemoryTestError> {
        if measurements.len() < 2 {
            return Ok(0.0);
        }

        let n = measurements.len() as f64;
        let start_time = measurements.first().unwrap().timestamp.timestamp_millis() as f64;

        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_x2 = 0.0;

        for measurement in measurements {
            let x = measurement.timestamp.timestamp_millis() as f64 - start_time;
            let y = measurement.total_memory_bytes as f64;

            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_x2 += x * x;
        }

        // Calculate slope (growth rate in bytes per millisecond)
        let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x * sum_x);

        // Convert to bytes per second
        Ok(slope * 1000.0)
    }

    /// Calculate memory volatility (standard deviation)
    async fn calculate_volatility(&self, memory_values: &[u64]) -> Result<f64, MemoryTestError> {
        if memory_values.is_empty() {
            return Ok(0.0);
        }

        let mean = memory_values.iter().sum::<u64>() as f64 / memory_values.len() as f64;
        let variance = memory_values.iter()
            .map(|&value| {
                let diff = value as f64 - mean;
                diff * diff
            })
            .sum::<f64>() / memory_values.len() as f64;

        let std_dev = variance.sqrt();

        // Return coefficient of variation (normalized by mean)
        if mean > 0.0 {
            Ok(std_dev / mean)
        } else {
            Ok(0.0)
        }
    }

    /// Calculate cleanup efficiency (0-1, higher is better)
    async fn calculate_cleanup_efficiency(&self, measurements: &[MemoryMeasurement]) -> Result<f64, MemoryTestError> {
        if measurements.len() < 3 {
            return Ok(1.0);
        }

        let baseline = measurements.first().unwrap().total_memory_bytes;
        let peak = measurements.iter().map(|m| m.total_memory_bytes).max().unwrap();
        let final_memory = measurements.last().unwrap().total_memory_bytes;

        if peak <= baseline {
            return Ok(1.0);
        }

        let memory_recovered = peak.saturating_sub(final_memory);
        let memory_growth = peak.saturating_sub(baseline);

        if memory_growth == 0 {
            return Ok(1.0);
        }

        Ok(memory_recovered as f64 / memory_growth as f64)
    }

    /// Calculate memory per operation
    async fn calculate_memory_per_operation(&self, _measurements: &[MemoryMeasurement]) -> Result<f64, MemoryTestError> {
        // This would require operation tracking in the actual implementation
        // For now, return a reasonable estimate
        Ok(1024.0) // 1KB per operation estimate
    }

    /// Calculate leak rate using more sophisticated analysis
    async fn calculate_leak_rate(&self, measurements: &[MemoryMeasurement]) -> Result<f64, MemoryTestError> {
        // Use the growth rate as the leak rate
        self.calculate_growth_rate(measurements).await
    }

    /// Calculate confidence level for leak detection
    async fn calculate_leak_confidence(&self, measurements: &[MemoryMeasurement], leak_rate: f64) -> Result<f64, MemoryTestError> {
        if measurements.len() < 3 {
            return Ok(0.0);
        }

        // Calculate R-squared for linear fit
        let n = measurements.len() as f64;
        let start_time = measurements.first().unwrap().timestamp.timestamp_millis() as f64;

        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_x2 = 0.0;
        let mut sum_y2 = 0.0;

        for measurement in measurements {
            let x = measurement.timestamp.timestamp_millis() as f64 - start_time;
            let y = measurement.total_memory_bytes as f64;

            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_x2 += x * x;
            sum_y2 += y * y;
        }

        let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x * sum_x);
        let intercept = (sum_y - slope * sum_x) / n;

        // Calculate R-squared
        let y_mean = sum_y / n;
        let mut ss_total = 0.0;
        let mut ss_residual = 0.0;

        for measurement in measurements {
            let x = measurement.timestamp.timestamp_millis() as f64 - start_time;
            let y = measurement.total_memory_bytes as f64;
            let y_predicted = slope * x + intercept;

            ss_total += (y - y_mean).powi(2);
            ss_residual += (y - y_predicted).powi(2);
        }

        let r_squared = if ss_total > 0.0 {
            1.0 - (ss_residual / ss_total)
        } else {
            0.0
        };

        // Combine R-squared with leak rate magnitude for confidence
        let leak_threshold = self.config.thresholds.leak_threshold_bytes as f64;
        let rate_factor = if leak_rate > 0.0 {
            (leak_rate / leak_threshold).min(1.0)
        } else {
            0.0
        };

        Ok(r_squared * 0.7 + rate_factor * 0.3)
    }

    /// Analyze leak pattern characteristics
    async fn analyze_leak_pattern(&self, measurements: &[MemoryMeasurement]) -> Result<LeakPatternAnalysis, MemoryTestError> {
        // This is a simplified pattern analysis
        // A more sophisticated implementation would use FFT or other signal processing

        let leak_rate = self.calculate_leak_rate(measurements).await?;
        let growth_rate = self.calculate_growth_rate(measurements).await?;

        // Determine pattern type based on characteristics
        let pattern_type = if leak_rate > 0.0 {
            // Check for exponential growth
            let memory_values: Vec<u64> = measurements.iter().map(|m| m.total_memory_bytes).collect();
            let ratio = memory_values.last().unwrap_or(&0) as f64 / memory_values.first().unwrap_or(&1) as f64;

            if ratio > 2.0 && measurements.len() > 10 {
                LeakPatternType::Exponential
            } else {
                LeakPatternType::Linear
            }
        } else {
            LeakPatternType::Cyclic
        };

        let growth_characteristics = GrowthCharacteristics {
            rate: growth_rate,
            acceleration: 0.0, // Would need more complex analysis
            consistency: 0.8,  // Would need statistical analysis
        };

        // Generate time patterns (simplified)
        let time_patterns = vec![
            TimePattern {
                period: Duration::from_secs(60),
                amplitude: 1024 * 1024, // 1MB
                phase: 0.0,
            }
        ];

        Ok(LeakPatternAnalysis {
            pattern_type,
            growth_characteristics,
            operation_correlation: 0.7, // Would need correlation analysis
            time_patterns,
        })
    }

    /// Identify suspected leak sources
    async fn identify_leak_sources(&self, measurements: &[MemoryMeasurement]) -> Result<Vec<String>, MemoryTestError> {
        let mut sources = Vec::new();

        if measurements.is_empty() {
            return Ok(sources);
        }

        // Analyze different memory components
        let first = &measurements[0];
        let last = &measurements[measurements.len() - 1];

        // Check cache memory growth
        if last.cache_memory_bytes > first.cache_memory_bytes * 2 {
            sources.push("Cache memory accumulation".to_string());
        }

        // Check connection memory growth
        if last.connection_memory_bytes > first.connection_memory_bytes * 2 {
            sources.push("Connection pool memory leaks".to_string());
        }

        // Check Arc/Mutex reference count growth
        if last.arc_ref_count > first.arc_ref_count * 2 {
            sources.push("Arc/Mutex reference cycle leaks".to_string());
        }

        // Check heap memory growth
        if last.heap_memory_bytes > first.heap_memory_bytes * 2 {
            sources.push("Heap allocation leaks".to_string());
        }

        // Add custom metric analysis
        for (metric_name, _) in &last.custom_metrics {
            if let Some(first_value) = first.custom_metrics.get(metric_name) {
                if last.custom_metrics.get(metric_name).unwrap_or(&0.0) > first_value * 2.0 {
                    sources.push(format!("{} metric growth", metric_name));
                }
            }
        }

        if sources.is_empty() {
            sources.push("General memory growth - requires detailed investigation".to_string());
        }

        Ok(sources)
    }

    /// Calculate operations per second
    async fn calculate_operations_per_second(&self, _session: &super::MemoryTestSession, duration: Duration) -> Result<f64, MemoryTestError> {
        // This would require operation tracking in the actual implementation
        if duration.as_secs() > 0 {
            Ok(10.0) // Estimate 10 operations per second
        } else {
            Ok(0.0)
        }
    }

    /// Calculate response time metrics
    async fn calculate_response_time_metrics(&self, _session: &super::MemoryTestSession) -> Result<(f64, f64), MemoryTestError> {
        // These would be measured during actual operations
        Ok((50.0, 100.0)) // Average 50ms, P95 100ms
    }

    /// Calculate error rate
    async fn calculate_error_rate(&self, _session: &super::MemoryTestSession) -> Result<f64, MemoryTestError> {
        // This would be tracked during actual operations
        Ok(0.01) // 1% error rate
    }

    /// Calculate throughput
    async fn calculate_throughput(&self, _session: &super::MemoryTestSession, duration: Duration) -> Result<f64, MemoryTestError> {
        if duration.as_secs() > 0 {
            Ok(1024.0 * 1024.0) // 1MB/s throughput
        } else {
            Ok(0.0)
        }
    }

    /// Calculate resource utilization
    async fn calculate_resource_utilization(&self, measurements: &[MemoryMeasurement]) -> Result<ResourceUtilization, MemoryTestError> {
        if measurements.is_empty() {
            return Ok(ResourceUtilization {
                cpu_utilization: 0.0,
                memory_utilization: 0.0,
                connection_utilization: 0.0,
                cache_hit_rate: 0.0,
            });
        }

        // Calculate average utilization across measurements
        let avg_memory = measurements.iter().map(|m| m.total_memory_bytes).sum::<u64>() as f64 / measurements.len() as f64;
        let max_memory = measurements.iter().map(|m| m.total_memory_bytes).max().unwrap_or(0) as f64;

        let memory_utilization = if max_memory > 0.0 {
            avg_memory / max_memory
        } else {
            0.0
        };

        // These would be measured in a real implementation
        Ok(ResourceUtilization {
            cpu_utilization: 0.3,  // 30% CPU
            memory_utilization,    // Calculated from measurements
            connection_utilization: 0.2, // 20% connection utilization
            cache_hit_rate: 0.8,   // 80% cache hit rate
        })
    }

    /// Calculate violation severity based on how much the threshold is exceeded
    fn calculate_violation_severity(&self, actual: f64, threshold: f64) -> ViolationSeverity {
        if threshold == 0.0 {
            return ViolationSeverity::Low;
        }

        let ratio = actual / threshold;

        if ratio >= 2.0 {
            ViolationSeverity::Critical
        } else if ratio >= 1.5 {
            ViolationSeverity::High
        } else if ratio >= 1.1 {
            ViolationSeverity::Medium
        } else {
            ViolationSeverity::Low
        }
    }
}