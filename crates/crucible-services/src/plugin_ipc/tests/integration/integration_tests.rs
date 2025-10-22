//! # Integration Tests
//!
//! Comprehensive end-to-end integration tests for the IPC protocol system,
//! testing complete workflows, multi-plugin scenarios, security integration,
//! performance benchmarks, failure recovery, and real-world usage patterns.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::plugin_ipc::{
    protocol::ProtocolHandler,
    security::SecurityManager,
    transport::TransportManager,
    client::IpcClient,
    server::IpcServer,
    metrics::MetricsCollector,
    message::{IpcMessage, MessageType, MessagePayload, ClientCapabilities},
    config::IpcConfig,
    error::IpcError,
};

use super::common::{
    *,
    fixtures::*,
    mocks::*,
    helpers::*,
};

/// End-to-end client-server communication tests
pub struct ClientServerIntegrationTests;

impl ClientServerIntegrationTests {
    /// Test complete client-server communication workflow
    pub async fn test_complete_workflow() -> IpcResult<()> {
        // Create test components
        let config = ConfigFixtures::fast_ipc();
        let security_manager = Arc::new(MockSecurityManager::new());
        let transport_manager = Arc::new(MockTransportManager::new());
        let metrics = Arc::new(MockMetricsCollector::new());

        // Create server
        let server = MockIpcServer::new(
            config.clone(),
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        );
        server.start().await?;

        // Create client
        let client = MockIpcClient::new(
            config,
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        );
        client.connect().await?;

        // Authenticate
        let token = security_manager.generate_token("test_user", vec!["read", "write"]).await?;
        client.authenticate(&token).await?;

        // Send request
        let request = MessageFixtures::request_to("test_plugin", "test_operation", json!({"data": "test"}));
        let response = client.send_request(request).await?;

        // Verify response
        assert!(matches!(response.header.message_type, MessageType::Response));
        if let MessagePayload::Response(resp) = response.payload {
            assert!(resp.success);
        }

        // Cleanup
        client.disconnect().await?;
        server.stop().await?;

        Ok(())
    }

    /// Test concurrent client operations
    pub async fn test_concurrent_clients() -> IpcResult<()> {
        let config = ConfigFixtures::fast_ipc();
        let security_manager = Arc::new(MockSecurityManager::new());
        let transport_manager = Arc::new(MockTransportManager::new());
        let metrics = Arc::new(MockMetricsCollector::new());

        // Create server
        let server = Arc::new(MockIpcServer::new(
            config.clone(),
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        ));
        server.start().await?;

        let num_clients = 50;
        let requests_per_client = 10;

        // Create multiple clients
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_clients,
            |client_id| {
                let server_config = config.clone();
                let security_manager = Arc::clone(&security_manager);
                let transport_manager = Arc::clone(&transport_manager);
                let metrics = Arc::clone(&metrics);
                async move {
                    // Create client
                    let client = MockIpcClient::new(
                        server_config,
                        security_manager,
                        transport_manager,
                        metrics,
                    );
                    client.connect().await?;

                    // Authenticate
                    let token = client.generate_token(&format!("user_{}", client_id)).await?;
                    client.authenticate(&token).await?;

                    // Send multiple requests
                    let mut success_count = 0;
                    for req_id in 0..requests_per_client {
                        let request = MessageFixtures::request_to(
                            "test_plugin",
                            &format!("operation_{}_{}", client_id, req_id),
                            json!({"client_id": client_id, "request_id": req_id}),
                        );

                        match client.send_request(request).await {
                            Ok(response) => {
                                if let MessagePayload::Response(resp) = response.payload {
                                    if resp.success {
                                        success_count += 1;
                                    }
                                }
                            }
                            Err(_) => continue,
                        }
                    }

                    client.disconnect().await?;
                    Ok(success_count)
                }
            },
        ).await;

        // Verify results
        let mut total_successes = 0;
        for result in results {
            if let Ok(successes) = result {
                total_successes += successes;
            }
        }

        let expected_total = num_clients * requests_per_client;
        assert!(total_successes >= expected_total / 2); // At least 50% success rate

        // Cleanup
        server.stop().await?;

        Ok(())
    }

    /// Test connection resilience
    pub async fn test_connection_resilience() -> IpcResult<()> {
        let config = ConfigFixtures::fast_ipc();
        let security_manager = Arc::new(MockSecurityManager::new());
        let transport_manager = Arc::new(MockTransportManager::new());
        let metrics = Arc::new(MockMetricsCollector::new());

        // Create server with failure simulation
        let server = Arc::new(MockIpcServer::new(
            config.clone(),
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        ));
        server.start().await?;

        let client = MockIpcClient::new(
            config,
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        );
        client.connect().await?;

        // Send initial requests
        for i in 0..10 {
            let request = MessageFixtures::request("test", json!({"seq": i}));
            let _response = client.send_request(request).await?;
        }

        // Simulate connection failure
        transport_manager.set_failure(true).await;

        // Requests should fail
        let result = client.send_request(MessageFixtures::request("test", json!({}))).await;
        assert!(result.is_err());

        // Restore connection
        transport_manager.set_failure(false).await;

        // Client should automatically reconnect
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Requests should succeed again
        let request = MessageFixtures::request("test", json!({"reconnected": true}));
        let response = client.send_request(request).await?;
        assert!(matches!(response.header.message_type, MessageType::Response));

        // Cleanup
        client.disconnect().await?;
        server.stop().await?;

        Ok(())
    }
}

/// Multi-plugin scenario tests
pub struct MultiPluginTests;

impl MultiPluginTests {
    /// Test multiple plugins with different capabilities
    pub async fn test_multiple_plugin_types() -> IpcResult<()> {
        let config = ConfigFixtures::fast_ipc();
        let security_manager = Arc::new(MockSecurityManager::new());
        let transport_manager = Arc::new(MockTransportManager::new());
        let metrics = Arc::new(MockMetricsCollector::new());

        // Create server with multiple plugin handlers
        let server = Arc::new(MockIpcServer::new(
            config.clone(),
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        ));

        // Register different plugin types
        server.register_plugin("text_processor", PluginCapabilities {
            plugin_type: "text".to_string(),
            version: "1.0.0".to_string(),
            operations: vec!["read".to_string(), "write".to_string(), "transform".to_string()],
            data_formats: vec!["json".to_string(), "text".to_string()],
            max_concurrent_requests: 5,
            supports_streaming: false,
            supports_batching: true,
            supports_compression: true,
            supports_encryption: true,
            resource_requirements: HashMap::new(),
            metadata: HashMap::new(),
        }).await?;

        server.register_plugin("image_processor", PluginCapabilities {
            plugin_type: "image".to_string(),
            version: "2.0.0".to_string(),
            operations: vec!["resize".to_string(), "crop".to_string(), "filter".to_string()],
            data_formats: vec!["png".to_string(), "jpg".to_string(), "webp".to_string()],
            max_concurrent_requests: 2,
            supports_streaming: true,
            supports_batching: false,
            supports_compression: true,
            supports_encryption: true,
            resource_requirements: HashMap::new(),
            metadata: HashMap::new(),
        }).await?;

        server.start().await?;

        // Create client
        let client = MockIpcClient::new(
            config,
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        );
        client.connect().await?;

        // Authenticate
        let token = security_manager.generate_token("multi_user", vec!["read", "write", "transform"]).await?;
        client.authenticate(&token).await?;

        // Test text processing
        let text_request = MessageFixtures::request_to(
            "text_processor",
            "transform",
            json!({"text": "Hello, world!", "operation": "uppercase"}),
        );
        let text_response = client.send_request(text_request).await?;
        assert!(matches!(text_response.header.message_type, MessageType::Response));

        // Test image processing
        let image_request = MessageFixtures::request_to(
            "image_processor",
            "resize",
            json!({"width": 800, "height": 600, "format": "png"}),
        );
        let image_response = client.send_request(image_request).await?;
        assert!(matches!(image_response.header.message_type, MessageType::Response));

        // Cleanup
        client.disconnect().await?;
        server.stop().await?;

        Ok(())
    }

    /// Test plugin coordination and message routing
    pub async fn test_plugin_coordination() -> IpcResult<()> {
        let config = ConfigFixtures::fast_ipc();
        let security_manager = Arc::new(MockSecurityManager::new());
        let transport_manager = Arc::new(MockTransportManager::new());
        let metrics = Arc::new(MockMetricsCollector::new());

        let server = Arc::new(MockIpcServer::new(
            config.clone(),
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        ));

        // Register coordinated plugins
        server.register_plugin("data_collector", PluginCapabilities {
            plugin_type: "collector".to_string(),
            version: "1.0.0".to_string(),
            operations: vec!["collect".to_string()],
            data_formats: vec!["json".to_string()],
            max_concurrent_requests: 10,
            supports_streaming: true,
            supports_batching: true,
            supports_compression: true,
            supports_encryption: true,
            resource_requirements: HashMap::new(),
            metadata: HashMap::new(),
        }).await?;

        server.register_plugin("data_processor", PluginCapabilities {
            plugin_type: "processor".to_string(),
            version: "1.0.0".to_string(),
            operations: vec!["process".to_string()],
            data_formats: vec!["json".to_string()],
            max_concurrent_requests: 5,
            supports_streaming: true,
            supports_batching: true,
            supports_compression: true,
            supports_encryption: true,
            resource_requirements: HashMap::new(),
            metadata: HashMap::new(),
        }).await?;

        server.register_plugin("data_storage", PluginCapabilities {
            plugin_type: "storage".to_string(),
            version: "1.0.0".to_string(),
            operations: vec!["store".to_string()],
            data_formats: vec!["json".to_string()],
            max_concurrent_requests: 3,
            supports_streaming: false,
            supports_batching: true,
            supports_compression: true,
            supports_encryption: true,
            resource_requirements: HashMap::new(),
            metadata: HashMap::new(),
        }).await?;

        server.start().await?;

        let client = MockIpcClient::new(
            config,
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        );
        client.connect().await?;

        let token = security_manager.generate_token("pipeline_user", vec!["collect", "process", "store"]).await?;
        client.authenticate(&token).await?;

        // Execute pipeline: collect -> process -> store
        let collect_request = MessageFixtures::request_to(
            "data_collector",
            "collect",
            json!({"source": "test_data", "format": "json"}),
        );
        let collect_response = client.send_request(collect_request).await?;

        if let MessagePayload::Response(resp) = collect_response.payload {
            let data_id = resp.data.as_ref()
                .and_then(|d| d.get("data_id"))
                .and_then(|id| id.as_str())
                .unwrap_or("unknown");

            // Process collected data
            let process_request = MessageFixtures::request_to(
                "data_processor",
                "process",
                json!({"data_id": data_id, "operations": ["normalize", "validate"]}),
            );
            let process_response = client.send_request(process_request).await?;

            if let MessagePayload::Response(resp) = process_response.payload {
                let processed_id = resp.data.as_ref()
                    .and_then(|d| d.get("processed_id"))
                    .and_then(|id| id.as_str())
                    .unwrap_or("unknown");

                // Store processed data
                let store_request = MessageFixtures::request_to(
                    "data_storage",
                    "store",
                    json!({"processed_id": processed_id, "location": "primary_storage"}),
                );
                let store_response = client.send_request(store_request).await?;
                assert!(matches!(store_response.header.message_type, MessageType::Response));
            }
        }

        // Cleanup
        client.disconnect().await?;
        server.stop().await?;

        Ok(())
    }
}

/// Security integration tests
pub struct SecurityIntegrationTests;

impl SecurityIntegrationTests {
    /// Test end-to-end security workflow
    pub async fn test_security_workflow() -> IpcResult<()> {
        let config = ConfigFixtures::basic_ipc();
        let security_manager = Arc::new(MockSecurityManager::new());
        let transport_manager = Arc::new(MockTransportManager::new());
        let metrics = Arc::new(MockMetricsCollector::new());

        // Create server with security enabled
        let server = Arc::new(MockIpcServer::new(
            config.clone(),
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        ));
        server.start().await?;

        // Create client
        let client = MockIpcClient::new(
            config,
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        );

        // Test connection without authentication (should fail)
        client.connect().await?;
        let result = client.send_request(MessageFixtures::request("test", json!({}))).await;
        assert!(result.is_err());

        // Authenticate with valid token
        let token = security_manager.generate_token("secure_user", vec!["read", "write"]).await?;
        client.authenticate(&token).await?;

        // Send encrypted message
        let mut secure_message = MessageFixtures::request("secure_operation", json!({"sensitive": "data"}));
        secure_message.header.flags.encrypted = true;

        let response = client.send_request(secure_message).await?;
        assert!(matches!(response.header.message_type, MessageType::Response));

        // Test authorization failure
        let unauthorized_request = MessageFixtures::request("admin_operation", json!({}));
        let result = client.send_request(unauthorized_request).await;
        assert!(result.is_err());

        // Cleanup
        client.disconnect().await?;
        server.stop().await?;

        Ok(())
    }

    /// Test session management and token refresh
    pub async fn test_session_management() -> IpcResult<()> {
        let config = ConfigFixtures::basic_ipc();
        let security_manager = Arc::new(MockSecurityManager::new());
        let transport_manager = Arc::new(MockTransportManager::new());
        let metrics = Arc::new(MockMetricsCollector::new());

        let server = Arc::new(MockIpcServer::new(
            config.clone(),
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        ));
        server.start().await?;

        let client = MockIpcClient::new(
            config,
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        );

        client.connect().await?;

        // Initial authentication
        let token = security_manager.generate_token("session_user", vec!["read"]).await?;
        client.authenticate(&token).await?;

        // Send requests with current session
        for i in 0..5 {
            let request = MessageFixtures::request("test", json!({"session": i}));
            let _response = client.send_request(request).await?;
        }

        // Simulate token expiry and refresh
        let refreshed_token = security_manager.refresh_token(&token).await?;
        client.authenticate(&refreshed_token).await?;

        // Continue with refreshed session
        for i in 5..10 {
            let request = MessageFixtures::request("test", json!({"refreshed": i}));
            let _response = client.send_request(request).await?;
        }

        // Cleanup
        client.disconnect().await?;
        server.stop().await?;

        Ok(())
    }

    /// Test secure multiplexing
    pub async fn test_secure_multiplexing() -> IpcResult<()> {
        let config = ConfigFixtures::basic_ipc();
        let security_manager = Arc::new(MockSecurityManager::new());
        let transport_manager = Arc::new(MockTransportManager::new());
        let metrics = Arc::new(MockMetricsCollector::new());

        let server = Arc::new(MockIpcServer::new(
            config.clone(),
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        ));
        server.start().await?;

        let client = MockIpcClient::new(
            config,
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        );

        client.connect().await?;
        let token = security_manager.generate_token("multiplex_user", vec!["read", "write"]).await?;
        client.authenticate(&token).await?;

        // Create multiple secure streams
        let num_streams = 5;
        let mut stream_ids = Vec::new();

        for i in 0..num_streams {
            let stream_id = format!("secure_stream_{}", i);
            stream_ids.push(stream_id.clone());

            // Send stream initialization
            let init_request = MessageFixtures::request_to(
                "stream_manager",
                "init_stream",
                json!({"stream_id": stream_id, "encryption": true}),
            );
            let _response = client.send_request(init_request).await?;
        }

        // Send messages on all streams
        for stream_id in &stream_ids {
            for msg_num in 0..3 {
                let message = MessageFixtures::request_to(
                    "stream_handler",
                    "stream_message",
                    json!({
                        "stream_id": stream_id,
                        "message_num": msg_num,
                        "data": format!("Secure data for {} message {}", stream_id, msg_num)
                    }),
                );
                let _response = client.send_request(message).await?;
            }
        }

        // Cleanup
        client.disconnect().await?;
        server.stop().await?;

        Ok(())
    }
}

/// Performance benchmarking tests
pub struct PerformanceBenchmarks;

impl PerformanceBenchmarks {
    /// Benchmark throughput under load
    pub async fn benchmark_throughput() -> IpcResult<(f64, f64)> {
        let config = ConfigFixtures::fast_ipc();
        let security_manager = Arc::new(MockSecurityManager::new());
        let transport_manager = Arc::new(MockTransportManager::new());
        let metrics = Arc::new(MockMetricsCollector::new());

        // Optimize for performance
        transport_manager.set_delay(Duration::from_millis(1), Duration::from_millis(1), Duration::from_millis(1)).await;

        let server = Arc::new(MockIpcServer::new(
            config.clone(),
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        ));
        server.start().await?;

        let client = MockIpcClient::new(
            config,
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        );

        client.connect().await?;
        let token = security_manager.generate_token("perf_user", vec!["read", "write"]).await?;
        client.authenticate(&token).await?;

        let num_requests = 1000;
        let message_size = 1024; // 1KB messages

        // Benchmark request-response throughput
        let start = SystemTime::now();
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_requests,
            |i| {
                let client = &client;
                async move {
                    let request = MessageFixtures::request(
                        "perf_test",
                        json!({
                            "data": "x".repeat(message_size),
                            "request_id": i
                        }),
                    );
                    client.send_request(request).await
                }
            },
        ).await;
        let duration = start.elapsed().unwrap();

        // Calculate throughput
        let total_requests = results.len();
        let successful_requests = results.iter().filter(|r| r.is_ok()).count();
        let requests_per_sec = total_requests as f64 / duration.as_secs_f64();
        let success_rate = successful_requests as f64 / total_requests as f64;

        // Verify reasonable performance
        assert!(requests_per_sec > 100.0); // At least 100 req/sec
        assert!(success_rate > 0.95); // At least 95% success rate

        // Cleanup
        client.disconnect().await?;
        server.stop().await?;

        Ok((requests_per_sec, success_rate))
    }

    /// Benchmark latency measurements
    pub async fn benchmark_latency() -> IpcResult<Duration> {
        let config = ConfigFixtures::fast_ipc();
        let security_manager = Arc::new(MockSecurityManager::new());
        let transport_manager = Arc::new(MockTransportManager::new());
        let metrics = Arc::new(MockMetricsCollector::new());

        // Set minimal delays for latency testing
        transport_manager.set_delay(Duration::from_millis(1), Duration::from_millis(1), Duration::from_millis(1)).await;

        let server = Arc::new(MockIpcServer::new(
            config.clone(),
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        ));
        server.start().await?;

        let client = MockIpcClient::new(
            config,
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        );

        client.connect().await?;
        let token = security_manager.generate_token("latency_user", vec!["ping"]).await?;
        client.authenticate(&token).await?;

        let num_samples = 100;
        let mut total_latency = Duration::ZERO;

        for i in 0..num_samples {
            let start = SystemTime::now();

            let request = MessageFixtures::request(
                "ping",
                json!({"timestamp": start.duration_since(UNIX_EPOCH).unwrap().as_millis(), "sample": i}),
            );
            let _response = client.send_request(request).await?;

            let latency = start.elapsed().unwrap();
            total_latency += latency;
        }

        let average_latency = total_latency / num_samples;

        // Cleanup
        client.disconnect().await?;
        server.stop().await?;

        Ok(average_latency)
    }

    /// Benchmark concurrent performance
    pub async fn benchmark_concurrent_performance() -> IpcResult<f64> {
        let config = ConfigFixtures::fast_ipc();
        let security_manager = Arc::new(MockSecurityManager::new());
        let transport_manager = Arc::new(MockTransportManager::new());
        let metrics = Arc::new(MockMetricsCollector::new());

        let server = Arc::new(MockIpcServer::new(
            config.clone(),
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        ));
        server.start().await?;

        let num_clients = 20;
        let requests_per_client = 50;

        // Run concurrent clients
        let start = SystemTime::now();
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_clients,
            |client_id| {
                let server_config = config.clone();
                let security_manager = Arc::clone(&security_manager);
                let transport_manager = Arc::clone(&transport_manager);
                let metrics = Arc::clone(&metrics);
                async move {
                    let client = MockIpcClient::new(
                        server_config,
                        security_manager,
                        transport_manager,
                        metrics,
                    );

                    client.connect().await?;
                    let token = client.generate_token(&format!("client_{}", client_id)).await?;
                    client.authenticate(&token).await?;

                    let mut success_count = 0;
                    for req_id in 0..requests_per_client {
                        let request = MessageFixtures::request(
                            "concurrent_test",
                            json!({"client_id": client_id, "request_id": req_id}),
                        );

                        if client.send_request(request).await.is_ok() {
                            success_count += 1;
                        }
                    }

                    client.disconnect().await?;
                    Ok(success_count)
                }
            },
        ).await;
        let duration = start.elapsed().unwrap();

        // Calculate metrics
        let total_requests = num_clients * requests_per_client;
        let successful_requests: usize = results.iter().filter_map(|r| r.as_ref().ok()).sum();
        let requests_per_sec = total_requests as f64 / duration.as_secs_f64();
        let success_rate = successful_requests as f64 / total_requests as f64;

        // Verify concurrent performance
        assert!(requests_per_sec > 50.0); // At least 50 req/sec with concurrency
        assert!(success_rate > 0.90); // At least 90% success rate

        // Cleanup
        server.stop().await?;

        Ok(requests_per_sec)
    }
}

/// Real-world usage pattern tests
pub struct RealWorldScenarios;

impl RealWorldScenarios {
    /// Test document processing workflow
    pub async fn test_document_processing_workflow() -> IpcResult<()> {
        let config = ConfigFixtures::basic_ipc();
        let security_manager = Arc::new(MockSecurityManager::new());
        let transport_manager = Arc::new(MockTransportManager::new());
        let metrics = Arc::new(MockMetricsCollector::new());

        let server = Arc::new(MockIpcServer::new(
            config.clone(),
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        ));

        // Register document processing plugins
        server.register_plugin("ocr_processor", PluginCapabilities {
            plugin_type: "ocr".to_string(),
            version: "1.0.0".to_string(),
            operations: vec!["extract_text".to_string()],
            data_formats: vec!["pdf".to_string(), "tiff".to_string()],
            max_concurrent_requests: 3,
            supports_streaming: true,
            supports_batching: true,
            supports_compression: true,
            supports_encryption: true,
            resource_requirements: HashMap::new(),
            metadata: HashMap::new(),
        }).await?;

        server.register_plugin("nlp_processor", PluginCapabilities {
            plugin_type: "nlp".to_string(),
            version: "2.0.0".to_string(),
            operations: vec!["analyze_sentiment".to_string(), "extract_entities".to_string()],
            data_formats: vec!["json".to_string(), "text".to_string()],
            max_concurrent_requests: 5,
            supports_streaming: false,
            supports_batching: true,
            supports_compression: true,
            supports_encryption: true,
            resource_requirements: HashMap::new(),
            metadata: HashMap::new(),
        }).await?;

        server.start().await?;

        let client = MockIpcClient::new(
            config,
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        );

        client.connect().await?;
        let token = security_manager.generate_token("doc_user", vec!["read", "write", "analyze"]).await?;
        client.authenticate(&token).await?;

        // Simulate document processing workflow
        let document_data = base64::encode("Sample document content for OCR processing".as_bytes());

        // Step 1: OCR processing
        let ocr_request = MessageFixtures::request_to(
            "ocr_processor",
            "extract_text",
            json!({
                "document": document_data,
                "format": "pdf",
                "language": "en"
            }),
        );
        let ocr_response = client.send_request(ocr_request).await?;

        let extracted_text = if let MessagePayload::Response(resp) = ocr_response.payload {
            resp.data
                .and_then(|d| d.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("Sample text")
                .to_string()
        } else {
            "Sample text".to_string()
        };

        // Step 2: NLP analysis
        let nlp_request = MessageFixtures::request_to(
            "nlp_processor",
            "analyze_sentiment",
            json!({
                "text": extracted_text,
                "model": "sentiment_v2"
            }),
        );
        let nlp_response = client.send_request(nlp_request).await?;

        // Verify analysis completed
        assert!(matches!(nlp_response.header.message_type, MessageType::Response));
        if let MessagePayload::Response(resp) = nlp_response.payload {
            assert!(resp.success);
            assert!(resp.data.is_some());
        }

        // Cleanup
        client.disconnect().await?;
        server.stop().await?;

        Ok(())
    }

    /// Test real-time data processing pipeline
    pub async fn test_realtime_pipeline() -> IpcResult<()> {
        let config = ConfigFixtures::basic_ipc();
        let security_manager = Arc::new(MockSecurityManager::new());
        let transport_manager = Arc::new(MockTransportManager::new());
        let metrics = Arc::new(MockMetricsCollector::new());

        let server = Arc::new(MockIpcServer::new(
            config.clone(),
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        ));

        // Register real-time processing plugins
        server.register_plugin("data_ingestion", PluginCapabilities {
            plugin_type: "ingestion".to_string(),
            version: "1.0.0".to_string(),
            operations: vec!["ingest_stream".to_string()],
            data_formats: vec!["json".to_string(), "csv".to_string()],
            max_concurrent_requests: 10,
            supports_streaming: true,
            supports_batching: true,
            supports_compression: true,
            supports_encryption: true,
            resource_requirements: HashMap::new(),
            metadata: HashMap::new(),
        }).await?;

        server.register_plugin("data_transform", PluginCapabilities {
            plugin_type: "transform".to_string(),
            version: "1.0.0".to_string(),
            operations: vec!["transform".to_string(), "aggregate".to_string()],
            data_formats: vec!["json".to_string()],
            max_concurrent_requests: 8,
            supports_streaming: true,
            supports_batching: true,
            supports_compression: true,
            supports_encryption: true,
            resource_requirements: HashMap::new(),
            metadata: HashMap::new(),
        }).await?;

        server.start().await?;

        let client = MockIpcClient::new(
            config,
            Arc::clone(&security_manager),
            Arc::clone(&transport_manager),
            Arc::clone(&metrics),
        );

        client.connect().await?;
        let token = security_manager.generate_token("pipeline_user", vec!["read", "write", "stream"]).await?;
        client.authenticate(&token).await?;

        // Simulate real-time data stream
        let stream_id = Uuid::new_v4().to_string();
        let num_events = 100;

        // Start stream
        let start_stream_request = MessageFixtures::request_to(
            "data_ingestion",
            "ingest_stream",
            json!({
                "stream_id": stream_id,
                "format": "json",
                "batch_size": 10
            }),
        );
        let _start_response = client.send_request(start_stream_request).await?;

        // Send stream events
        for event_id in 0..num_events {
            let event = MessageFixtures::stream_chunk(
                &stream_id,
                event_id,
                json!({
                    "event_id": event_id,
                    "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis(),
                    "data": format!("Event data {}", event_id),
                    "source": "test_source"
                }).to_string().into_bytes(),
            );

            let _response = client.send_message(event).await?;

            // Small delay to simulate real-time
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // End stream
        let end_stream_request = MessageFixtures::stream_end(&stream_id, true, None);
        let _end_response = client.send_message(end_stream_request).await?;

        // Transform aggregated data
        let transform_request = MessageFixtures::request_to(
            "data_transform",
            "aggregate",
            json!({
                "stream_id": stream_id,
                "aggregations": ["count", "avg", "sum"],
                "group_by": ["source"]
            }),
        );
        let transform_response = client.send_request(transform_request).await?;

        // Verify transformation completed
        assert!(matches!(transform_response.header.message_type, MessageType::Response));
        if let MessagePayload::Response(resp) = transform_response.payload {
            assert!(resp.success);
        }

        // Cleanup
        client.disconnect().await?;
        server.stop().await?;

        Ok(())
    }
}

// Mock implementations for integration testing

#[derive(Debug)]
pub struct MockIpcServer {
    config: IpcConfig,
    security_manager: Arc<MockSecurityManager>,
    transport_manager: Arc<MockTransportManager>,
    metrics: Arc<MockMetricsCollector>,
    plugins: Arc<RwLock<HashMap<String, PluginCapabilities>>>,
    running: Arc<Mutex<bool>>,
}

#[derive(Debug)]
pub struct MockIpcClient {
    config: IpcConfig,
    security_manager: Arc<MockSecurityManager>,
    transport_manager: Arc<MockTransportManager>,
    metrics: Arc<MockMetricsCollector>,
    connection_id: Arc<Mutex<Option<String>>>,
    authenticated: Arc<Mutex<bool>>,
}

impl MockIpcServer {
    pub fn new(
        config: IpcConfig,
        security_manager: Arc<MockSecurityManager>,
        transport_manager: Arc<MockTransportManager>,
        metrics: Arc<MockMetricsCollector>,
    ) -> Self {
        Self {
            config,
            security_manager,
            transport_manager,
            metrics,
            plugins: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn start(&self) -> IpcResult<()> {
        *self.running.lock().await = true;
        Ok(())
    }

    pub async fn stop(&self) -> IpcResult<()> {
        *self.running.lock().await = false;
        Ok(())
    }

    pub async fn register_plugin(&self, name: String, capabilities: PluginCapabilities) -> IpcResult<()> {
        self.plugins.write().await.insert(name, capabilities);
        Ok(())
    }
}

impl MockIpcClient {
    pub fn new(
        config: IpcConfig,
        security_manager: Arc<MockSecurityManager>,
        transport_manager: Arc<MockTransportManager>,
        metrics: Arc<MockMetricsCollector>,
    ) -> Self {
        Self {
            config,
            security_manager,
            transport_manager,
            metrics,
            connection_id: Arc::new(Mutex::new(None)),
            authenticated: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn connect(&self) -> IpcResult<()> {
        let connection_id = self.transport_manager.connect("mock_server").await?;
        *self.connection_id.lock().await = Some(connection_id);
        Ok(())
    }

    pub async fn disconnect(&self) -> IpcResult<()> {
        if let Some(connection_id) = self.connection_id.lock().await.as_ref() {
            self.transport_manager.disconnect(connection_id).await?;
        }
        *self.connection_id.lock().await = None;
        *self.authenticated.lock().await = false;
        Ok(())
    }

    pub async fn authenticate(&self, token: &str) -> IpcResult<()> {
        let session_id = self.security_manager.authenticate(token).await?;
        *self.authenticated.lock().await = true;
        Ok(())
    }

    pub async fn generate_token(&self, user_id: &str) -> IpcResult<String> {
        self.security_manager.generate_token(user_id, vec!["read", "write"]).await
    }

    pub async fn send_request(&self, mut request: IpcMessage) -> IpcResult<IpcMessage> {
        if !*self.authenticated.lock().await {
            return Err(IpcError::Authentication {
                message: "Not authenticated".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::InvalidToken,
                retry_after: None,
            });
        }

        if let Some(connection_id) = self.connection_id.lock().await.as_ref() {
            self.transport_manager.send_message(connection_id, request.clone()).await?;

            // Receive response
            let response = self.transport_manager.receive_message(connection_id).await?;
            Ok(response)
        } else {
            Err(IpcError::Connection {
                message: "Not connected".to_string(),
                code: crate::plugin_ipc::error::ConnectionErrorCode::ConnectionClosed,
                endpoint: "mock_server".to_string(),
                retry_count: 0,
            })
        }
    }

    pub async fn send_message(&self, message: IpcMessage) -> IpcResult<()> {
        if !*self.authenticated.lock().await {
            return Err(IpcError::Authentication {
                message: "Not authenticated".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::InvalidToken,
                retry_after: None,
            });
        }

        if let Some(connection_id) = self.connection_id.lock().await.as_ref() {
            self.transport_manager.send_message(connection_id, message).await?;
            Ok(())
        } else {
            Err(IpcError::Connection {
                message: "Not connected".to_string(),
                code: crate::plugin_ipc::error::ConnectionErrorCode::ConnectionClosed,
                endpoint: "mock_server".to_string(),
                retry_count: 0,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async_test!(test_complete_workflow, {
        ClientServerIntegrationTests::test_complete_workflow().await.unwrap();
        "success"
    });

    async_test!(test_concurrent_clients, {
        ClientServerIntegrationTests::test_concurrent_clients().await.unwrap();
        "success"
    });

    async_test!(test_connection_resilience, {
        ClientServerIntegrationTests::test_connection_resilience().await.unwrap();
        "success"
    });

    async_test!(test_multiple_plugin_types, {
        MultiPluginTests::test_multiple_plugin_types().await.unwrap();
        "success"
    });

    async_test!(test_plugin_coordination, {
        MultiPluginTests::test_plugin_coordination().await.unwrap();
        "success"
    });

    async_test!(test_security_workflow, {
        SecurityIntegrationTests::test_security_workflow().await.unwrap();
        "success"
    });

    async_test!(test_session_management, {
        SecurityIntegrationTests::test_session_management().await.unwrap();
        "success"
    });

    async_test!(test_secure_multiplexing, {
        SecurityIntegrationTests::test_secure_multiplexing().await.unwrap();
        "success"
    });

    async_test!(test_throughput_benchmark, {
        let (throughput, success_rate) = PerformanceBenchmarks::benchmark_throughput().await.unwrap();
        assert!(throughput > 100.0);
        assert!(success_rate > 0.95);
        (throughput, success_rate)
    });

    async_test!(test_latency_benchmark, {
        let avg_latency = PerformanceBenchmarks::benchmark_latency().await.unwrap();
        assert!(avg_latency < Duration::from_millis(100)); // Less than 100ms
        avg_latency.as_millis()
    });

    async_test!(test_concurrent_performance_benchmark, {
        let throughput = PerformanceBenchmarks::benchmark_concurrent_performance().await.unwrap();
        assert!(throughput > 50.0);
        throughput
    });

    async_test!(test_document_processing_workflow, {
        RealWorldScenarios::test_document_processing_workflow().await.unwrap();
        "success"
    });

    async_test!(test_realtime_pipeline, {
        RealWorldScenarios::test_realtime_pipeline().await.unwrap();
        "success"
    });
}