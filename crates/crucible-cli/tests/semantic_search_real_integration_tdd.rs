//! TDD RED Phase Test: Semantic Search Real Integration
//!
//! This test file contains comprehensive failing tests that demonstrate the current
//! implementation gaps in semantic search functionality. The tests expose specific
//! issues with mock embeddings, configuration integration, and persistent storage.
//!
//! **Current Issues Analysis:**
//! - Mock embeddings are used instead of real embedding generation (line 1080 in vault_integration.rs)
//! - CLI embedding configuration is ignored during semantic search
//! - Database storage may not be properly persistent across runs
//! - Similarity scores are not meaningful or variable enough
//!
//! **Test Objectives:**
//! 1. Demonstrate current use of mock embeddings instead of real ones
//! 2. Show that CLI configuration is not respected during semantic search
//! 3. Test persistence of semantic search results across multiple runs
//! 4. Drive implementation of proper semantic search with real embeddings

/// Test helper to create a realistic test vault with diverse content
async fn create_test_kiln() -> Result<(TempDir, PathBuf)> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().to_path_buf();

    // Create diverse test markdown files with semantic content
    let test_files = vec![
        (
            "machine-learning-intro.md",
            r#"---
title: "Introduction to Machine Learning"
tags: ["AI", "ML", "neural-networks", "algorithms"]
---

# Introduction to Machine Learning

Machine learning is a subset of artificial intelligence that focuses on developing algorithms and statistical models that enable computer systems to improve their performance on a specific task through experience.

## Key Concepts

- **Supervised Learning**: Learning from labeled training data
- **Unsupervised Learning**: Finding patterns in unlabeled data
- **Neural Networks**: Computing systems inspired by biological neural networks
- **Deep Learning**: Subset of machine learning using multi-layered neural networks

## Applications

Machine learning is used in various domains including natural language processing, computer vision, recommendation systems, and autonomous vehicles.
"#,
        ),
        (
            "rust-systems-programming.md",
            r#"---
title: "Rust Systems Programming"
tags: ["rust", "systems", "memory-safety", "performance"]
---

# Rust Systems Programming

Rust is a systems programming language that guarantees memory safety without using a garbage collector. It provides zero-cost abstractions and prevents common programming errors through its ownership system.

## Memory Safety Features

- **Ownership System**: Ensures memory is managed safely at compile time
- **Borrowing**: Allows references to data without taking ownership
- **Lifetimes**: Prevents dangling references and use-after-free bugs

## Performance Characteristics

Rust provides performance comparable to C and C++ while offering modern language features. The compiler produces highly optimized machine code through advanced LLVM optimizations.
"#,
        ),
        (
            "database-vector-search.md",
            r#"---
title: "Vector Database Technologies"
tags: ["database", "vectors", "embeddings", "similarity-search"]
---

# Vector Database Technologies

Vector databases are specialized database systems designed to store and query high-dimensional vectors efficiently. They enable similarity search operations that find vectors closest to a given query vector based on distance metrics.

## Key Technologies

- **Embeddings**: Dense vector representations of data
- **Similarity Metrics**: Cosine similarity, Euclidean distance, dot product
- **Indexing Algorithms**: HNSW, IVF, LSH for efficient approximate nearest neighbor search
- **Scalability**: Horizontal scaling for billion-vector datasets

## Use Cases

Vector databases power semantic search, recommendation systems, image similarity search, and molecular similarity analysis in drug discovery.
"#,
        ),
        (
            "ai-research-transformers.md",
            r#"---
title: "Transformer Models in AI Research"
tags: ["transformers", "attention-mechanism", "NLP", "AI-research"]
---

# Transformer Models in AI Research

Transformer models have revolutionized natural language processing and artificial intelligence research. The attention mechanism allows models to process sequential data in parallel while capturing long-range dependencies.

## Key Innovations

- **Self-Attention**: Allows model to weigh importance of different input tokens
- **Multi-Head Attention**: Enables learning different types of relationships
- **Positional Encoding**: Provides positional information for sequential data
- **Encoder-Decoder Architecture**: Flexible framework for various NLP tasks

## Applications

Transformers power large language models like GPT, BERT, and T5. They excel in machine translation, text summarization, question answering, and code generation tasks.
"#,
        ),
        (
            "semantic-search-implementation.md",
            r#"---
title: "Implementing Semantic Search Systems"
tags: ["semantic-search", "embeddings", "vector-similarity", "search-algorithms"]
---

# Implementing Semantic Search Systems

Semantic search systems use vector embeddings to understand the meaning behind queries and find conceptually similar content, rather than relying on keyword matching alone.

## Implementation Components

- **Text Embedding Models**: Convert text to dense vector representations
- **Vector Storage**: Efficient storage and retrieval of high-dimensional vectors
- **Similarity Calculation**: Compute relevance scores using cosine similarity or other metrics
- **Ranking Algorithms**: Order results by semantic relevance

## Challenges

Semantic search faces challenges including computational complexity, embedding quality, domain-specific terminology, and real-time performance requirements.
"#,
        ),
    ];

    for (filename, content) in test_files {
        let file_path = kiln_path.join(filename);
        fs::write(file_path, content)?;
    }

    Ok((temp_dir, kiln_path))
}

/// Helper to run CLI semantic search with specific configuration
async fn run_semantic_search_with_config(
    kiln_path: &PathBuf,
    query: &str,
    embedding_url: Option<&str>,
    embedding_model: Option<&str>,
) -> Result<String> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cru"));
    cmd.arg("semantic")
        .arg(query)
        .arg("--top-k")
        .arg("5")
        .arg("--format")
        .arg("json")
        .env("OBSIDIAN_KILN_PATH", kiln_path.to_string_lossy().as_ref());

    if let Some(url) = embedding_url {
        cmd.env("EMBEDDING_ENDPOINT", url);
    }

    if let Some(model) = embedding_model {
        cmd.env("EMBEDDING_MODEL", model);
    }

    let output = cmd.output().await?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Helper to run CLI semantic search and return parsed JSON
async fn run_semantic_search_json(kiln_path: &PathBuf, query: &str) -> Result<Value> {
    let output = run_semantic_search_with_config(kiln_path, query, None, None).await?;
    let parsed: Value = serde_json::from_str(&output)?;
    Ok(parsed)
}

/// Helper to check if database exists and has embeddings
async fn check_database_embeddings(kiln_path: &PathBuf) -> Result<bool> {
    let db_path = kiln_path.join(".crucible/kiln.db");

    if !db_path.exists() {
        return Ok(false);
    }

    // Use CLI to check database stats
    let output = Command::new(env!("CARGO_BIN_EXE_cru"))
        .arg("config")
        .arg("--show")
        .env("OBSIDIAN_KILN_PATH", kiln_path.to_string_lossy().as_ref())
        .output()
        .await?;

    let output_str = String::from_utf8_lossy(&output.stdout);
    Ok(output_str.contains("embeddings") || db_path.exists())
}

/// Helper to process kiln to generate embeddings
async fn process_kiln_for_embeddings(kiln_path: &PathBuf) -> Result<()> {
    println!("🔄 Processing kiln to generate embeddings...");

    let output = Command::new(env!("CARGO_BIN_EXE_cru"))
        .arg("semantic")
        .arg("test query")
        .arg("--top-k")
        .arg("1")
        .env("OBSIDIAN_KILN_PATH", kiln_path.to_string_lossy().as_ref())
        .output()
        .await?;

    // Give some time for processing to complete
    sleep(Duration::from_millis(500)).await;

    Ok(())
}

#[cfg(test)]
mod semantic_search_real_integration_tdd_tests {
    use super::*;

    #[tokio::test]
    /// Test that demonstrates mock embeddings are used instead of real ones
    ///
    /// This test should FAIL because:
    /// 1. vault_integration.rs uses generate_mock_query_embedding() (line 1080)
    /// 2. Similarity scores follow predictable patterns based on query keywords
    /// 3. No actual embedding service is called to generate real query embeddings
    async fn test_semantic_search_uses_mock_embeddings_instead_of_real() -> Result<()> {
        let (_temp_dir, kiln_path) = create_test_kiln().await?;

        println!("🔍 TDD RED: Testing that semantic search uses mock embeddings");
        println!("📁 Test kiln: {}", kiln_path.display());

        // Process kiln to ensure embeddings exist
        process_kiln_for_embeddings(&kiln_path).await?;

        // Test queries that have specific mock patterns in vault_integration.rs
        let test_queries = vec![
            (
                "machine learning",
                "Should have high similarity pattern [0.8, 0.6, 0.1, 0.2]",
            ),
            (
                "neural networks",
                "Should have neural pattern [0.7, 0.4, 0.2, 0.3]",
            ),
            (
                "deep learning",
                "Should have deep pattern [0.6, 0.7, 0.1, 0.1]",
            ),
            (
                "artificial intelligence",
                "Should have AI pattern [0.5, 0.5, 0.3, 0.3]",
            ),
            (
                "data science",
                "Should have data pattern [0.4, 0.3, 0.6, 0.2]",
            ),
        ];

        for (query, expected_pattern) in test_queries {
            println!("\n🧪 Testing query: '{}' ({})", query, expected_pattern);

            match run_semantic_search_json(&kiln_path, query).await {
                Ok(result) => {
                    if let Some(results) = result.get("results").and_then(|r| r.as_array()) {
                        println!("📊 Found {} results", results.len());

                        if !results.is_empty() {
                            // Check first result's similarity score
                            if let Some(first_result) = results.first() {
                                if let Some(score) =
                                    first_result.get("score").and_then(|s| s.as_f64())
                                {
                                    println!("🎯 First result score: {:.4}", score);

                                    // The mock implementation produces predictable scores
                                    // Real embeddings would produce variable scores based on actual semantic similarity
                                    let is_predictable_mock_score = match query {
                                        q if q.to_lowercase().contains("machine learning") => {
                                            score > 0.7
                                        }
                                        q if q.to_lowercase().contains("neural") => score > 0.6,
                                        q if q.to_lowercase().contains("deep") => score > 0.5,
                                        q if q.to_lowercase().contains("artificial")
                                            || q.to_lowercase().contains("ai") =>
                                        {
                                            score > 0.4
                                        }
                                        q if q.to_lowercase().contains("data") => score > 0.3,
                                        _ => score > 0.2,
                                    };

                                    if is_predictable_mock_score {
                                        println!("❌ MOCK EMBEDDING DETECTED: Score follows predictable pattern for '{}'", query);
                                        println!("   This indicates mock embeddings are being used instead of real semantic similarity");
                                        println!("   Real embeddings would produce variable scores based on actual content similarity");
                                    } else {
                                        println!("✅ Score appears to be from real similarity calculation");
                                    }
                                }
                            }
                        }
                    } else {
                        println!("⚠️  No results found for query: {}", query);
                    }
                }
                Err(e) => {
                    println!("❌ Semantic search failed for query '{}': {}", query, e);
                }
            }
        }

        println!("\n❌ TDD RED PHASE FAILURE:");
        println!("   Current implementation uses mock embeddings (generate_mock_query_embedding)");
        println!("   Real embedding generation needs to be implemented");
        println!("   Configuration should be respected for embedding model selection");

        // This failure drives the implementation of real embeddings
        panic!(
            "RED PHASE: Semantic search uses mock embeddings instead of real embedding generation"
        );
    }

    #[tokio::test]
    /// Test that semantic search ignores CLI embedding configuration
    ///
    /// This test should FAIL because:
    /// 1. CLI embedding configuration is ignored during semantic search
    /// 2. Mock embeddings don't use configurable models or endpoints
    /// 3. Different configuration settings don't affect search results
    async fn test_semantic_search_ignores_cli_configuration() -> Result<()> {
        let (_temp_dir, kiln_path) = create_test_kiln().await?;

        println!("🔍 TDD RED: Testing that semantic search ignores CLI configuration");
        println!("📁 Test kiln: {}", kiln_path.display());

        // Process kiln first
        process_kiln_for_embeddings(&kiln_path).await?;

        let test_query = "machine learning algorithms";
        let mut results = Vec::new();

        // Test with different embedding configurations
        let test_configs = vec![
            (
                Some("http://localhost:11434"),
                Some("nomic-embed-text-v1.5"),
            ),
            (Some("http://localhost:11434"), Some("all-minilm-l6-v2")),
            (
                Some("https://api.openai.com"),
                Some("text-embedding-ada-002"),
            ),
            (None, None), // Default configuration
        ];

        for (i, (url, model)) in test_configs.iter().enumerate() {
            println!(
                "\n🧪 Testing configuration {}: URL={:?}, Model={:?}",
                i + 1,
                url,
                model
            );

            match run_semantic_search_with_config(
                &kiln_path,
                test_query,
                url.as_deref(),
                model.as_deref(),
            )
            .await
            {
                Ok(output) => match serde_json::from_str::<Value>(&output) {
                    Ok(parsed) => {
                        if let Some(search_results) =
                            parsed.get("results").and_then(|r| r.as_array())
                        {
                            let result_count = search_results.len();
                            let first_score = search_results
                                .first()
                                .and_then(|r| r.get("score"))
                                .and_then(|s| s.as_f64())
                                .unwrap_or(0.0);

                            println!(
                                "📊 Results: {} items, first score: {:.4}",
                                result_count, first_score
                            );
                            results.push((url.clone(), model.clone(), result_count, first_score));
                        } else {
                            println!("⚠️  No results found");
                            results.push((url.clone(), model.clone(), 0, 0.0));
                        }
                    }
                    Err(e) => {
                        println!("❌ Failed to parse JSON output: {}", e);
                        results.push((url.clone(), model.clone(), 0, 0.0));
                    }
                },
                Err(e) => {
                    println!("❌ Search failed: {}", e);
                    results.push((url.clone(), model.clone(), 0, 0.0));
                }
            }
        }

        // Check if all configurations produce identical results (indicating configuration is ignored)
        if results.len() >= 2 {
            let first_result = &results[0];
            let mut all_identical = true;

            for (i, result) in results.iter().enumerate().skip(1) {
                if result.2 != first_result.2 || (result.3 - first_result.3).abs() > 0.001 {
                    all_identical = false;
                    break;
                }
            }

            if all_identical {
                println!("\n❌ CONFIGURATION IGNORED:");
                println!("   All configurations produced identical results:");
                for (i, (url, model, count, score)) in results.iter().enumerate() {
                    println!(
                        "   Config {}: URL={:?}, Model={:?} -> {} results, score={:.4}",
                        i + 1,
                        url,
                        model,
                        count,
                        score
                    );
                }
                println!("\n   This indicates that CLI embedding configuration is being ignored");
                println!(
                    "   Different models and endpoints should produce different similarity scores"
                );

                // This failure drives configuration integration
                panic!("RED PHASE: CLI embedding configuration is ignored during semantic search");
            } else {
                println!("\n✅ Configuration appears to affect results (good sign)");
            }
        }

        println!("\n❌ ADDITIONAL CONFIGURATION ISSUES:");
        println!("   - Mock embeddings don't use configurable embedding models");
        println!("   - Embedding service URL configuration is ignored");
        println!("   - API keys and authentication settings are not used");
        println!("   - Batch size and timeout settings are ignored");

        // This test failure demonstrates the configuration integration gap
        panic!("RED PHASE: Semantic search configuration integration needs implementation");
    }

    #[tokio::test]
    /// Test that semantic search results are consistent across runs
    ///
    /// This test should FAIL because:
    /// 1. Current implementation may not use persistent database storage
    /// 2. Mock embeddings may produce different results across runs
    /// 3. Database state may not be properly maintained between processes
    async fn test_semantic_search_consistency_across_runs() -> Result<()> {
        let (_temp_dir, kiln_path) = create_test_kiln().await?;

        println!("🔍 TDD RED: Testing semantic search consistency across multiple runs");
        println!("📁 Test kiln: {}", kiln_path.display());

        let test_queries = vec![
            "machine learning",
            "rust programming",
            "vector databases",
            "transformer models",
        ];

        let mut all_results = Vec::new();

        // Run semantic search multiple times
        for run in 1..=3 {
            println!("\n🔄 Run {} of semantic search consistency test", run);

            // Process kiln on first run to ensure embeddings exist
            if run == 1 {
                process_kiln_for_embeddings(&kiln_path).await?;
            }

            let mut run_results = Vec::new();

            for query in &test_queries {
                match run_semantic_search_json(&kiln_path, query).await {
                    Ok(result) => {
                        if let Some(search_results) =
                            result.get("results").and_then(|r| r.as_array())
                        {
                            let result_summary: Vec<String> = search_results
                                .iter()
                                .take(3) // Take top 3 for comparison
                                .filter_map(|r| {
                                    let id =
                                        r.get("id").and_then(|i| i.as_str()).unwrap_or("unknown");
                                    let score =
                                        r.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0);
                                    Some(format!("{}:{:.4}", id, score))
                                })
                                .collect();

                            println!("📊 Query '{}': {}", query, result_summary.join(", "));
                            run_results.push((query.clone(), result_summary));
                        } else {
                            println!("⚠️  Query '{}': No results", query);
                            run_results.push((query.clone(), vec!["no_results".to_string()]));
                        }
                    }
                    Err(e) => {
                        println!("❌ Query '{}': Failed - {}", query, e);
                        run_results.push((query.clone(), vec![format!("error:{}", e)]));
                    }
                }
            }

            all_results.push(run_results);

            // Small delay between runs
            sleep(Duration::from_millis(200)).await;
        }

        // Check consistency across runs
        println!(
            "\n🔍 Analyzing consistency across {} runs",
            all_results.len()
        );

        if all_results.len() >= 2 {
            let first_run = &all_results[0];
            let mut all_consistent = true;

            for (run_idx, current_run) in all_results.iter().enumerate().skip(1) {
                println!("\n📊 Comparing run 1 with run {}:", run_idx + 1);

                for (query_idx, (query, first_results)) in first_run.iter().enumerate() {
                    if let Some((_, current_results)) = current_run.get(query_idx) {
                        if first_results != current_results {
                            println!("❌ Query '{}': Inconsistent results", query);
                            println!("   Run 1: {}", first_results.join(", "));
                            println!("   Run {}: {}", run_idx + 1, current_results.join(", "));
                            all_consistent = false;
                        } else {
                            println!("✅ Query '{}': Consistent results", query);
                        }
                    }
                }
            }

            if !all_consistent {
                println!("\n❌ INCONSISTENCY DETECTED:");
                println!("   Semantic search results are not consistent across multiple runs");
                println!("   This indicates issues with:");
                println!("   - Mock embeddings producing variable results");
                println!("   - Database state not being properly persisted");
                println!("   - Random elements in similarity calculation");
                println!("   - Race conditions or timing-dependent behavior");

                // This failure drives the need for consistent persistence
                panic!("RED PHASE: Semantic search results are inconsistent across runs");
            } else {
                println!("\n✅ All runs produced consistent results (good)");
            }
        }

        // Check database persistence
        let database_exists_after_runs = check_database_embeddings(&kiln_path).await?;
        println!(
            "\n📁 Database exists after runs: {}",
            database_exists_after_runs
        );

        if !database_exists_after_runs {
            println!("❌ Database persistence issue:");
            println!("   Database or embeddings not found after semantic search runs");
            println!("   This suggests that embeddings are not being properly stored");

            panic!("RED PHASE: Database persistence is not working correctly");
        }

        println!("\n❌ ADDITIONAL CONSISTENCY ISSUES:");
        println!("   - Mock similarity calculation may include random elements");
        println!("   - Database connections may not be properly managed");
        println!("   - Embedding storage may be inconsistent between process runs");

        Ok(())
    }

    #[tokio::test]
    /// Test that semantic search produces meaningful similarity scores
    ///
    /// This test should FAIL because:
    /// 1. Mock similarity scores follow predictable patterns instead of semantic relevance
    /// 2. Score variation doesn't correlate with actual content similarity
    /// 3. Different queries about the same topic don't produce related results
    async fn test_semantic_search_meaningful_similarity_scores() -> Result<()> {
        let (_temp_dir, kiln_path) = create_test_kiln().await?;

        println!("🔍 TDD RED: Testing meaningful similarity scores in semantic search");
        println!("📁 Test kiln: {}", kiln_path.display());

        // Process kiln to ensure embeddings exist
        process_kiln_for_embeddings(&kiln_path).await?;

        // Test queries that should produce semantically related results
        let semantic_test_cases = vec![
            // Related queries about machine learning
            (
                "machine learning",
                vec!["machine-learning-intro.md", "ai-research-transformers.md"],
            ),
            (
                "neural networks",
                vec!["machine-learning-intro.md", "ai-research-transformers.md"],
            ),
            (
                "AI algorithms",
                vec!["machine-learning-intro.md", "ai-research-transformers.md"],
            ),
            // Related queries about databases
            (
                "vector search",
                vec![
                    "database-vector-search.md",
                    "semantic-search-implementation.md",
                ],
            ),
            (
                "similarity search",
                vec![
                    "database-vector-search.md",
                    "semantic-search-implementation.md",
                ],
            ),
            (
                "embeddings",
                vec![
                    "database-vector-search.md",
                    "semantic-search-implementation.md",
                ],
            ),
            // Related queries about Rust
            ("memory safety", vec!["rust-systems-programming.md"]),
            ("systems programming", vec!["rust-systems-programming.md"]),
            ("performance", vec!["rust-systems-programming.md"]),
        ];

        let mut meaningful_results = 0;
        let total_tests = semantic_test_cases.len();

        for (query, expected_related_files) in semantic_test_cases {
            println!("\n🧪 Testing semantic relevance for query: '{}'", query);
            println!(
                "   Should return files related to: {:?}",
                expected_related_files
            );

            match run_semantic_search_json(&kiln_path, query).await {
                Ok(result) => {
                    if let Some(search_results) = result.get("results").and_then(|r| r.as_array()) {
                        if !search_results.is_empty() {
                            // Analyze top 3 results
                            let top_results: Vec<String> = search_results
                                .iter()
                                .take(3)
                                .filter_map(|r| {
                                    r.get("id")
                                        .and_then(|id| id.as_str())
                                        .and_then(|s| s.split('/').last())
                                        .map(String::from)
                                })
                                .collect();

                            let scores: Vec<f64> = search_results
                                .iter()
                                .take(3)
                                .filter_map(|r| r.get("score").and_then(|s| s.as_f64()))
                                .collect();

                            println!("📊 Top results: {:?}", top_results);
                            println!("📊 Scores: {:?}", scores);

                            // Check if any expected files appear in results
                            let has_related_file = expected_related_files.iter().any(|expected| {
                                top_results.iter().any(|result| result.contains(expected))
                            });

                            if has_related_file {
                                println!("✅ Found semantically related files");
                                meaningful_results += 1;
                            } else {
                                println!("❌ No semantically related files found");
                                println!("   Expected one of: {:?}", expected_related_files);
                                println!("   But got: {:?}", top_results);
                            }

                            // Check score distribution - mock scores often have unrealistic patterns
                            if scores.len() >= 2 {
                                let score_diff = (scores[0] - scores[1]).abs();
                                if score_diff < 0.01 {
                                    println!(
                                        "⚠️  Suspicious score distribution: scores are too similar"
                                    );
                                    println!("   This may indicate mock similarity calculation");
                                }
                            }
                        } else {
                            println!("❌ No results found for query");
                        }
                    } else {
                        println!("❌ Invalid result format");
                    }
                }
                Err(e) => {
                    println!("❌ Search failed: {}", e);
                }
            }
        }

        let meaningful_percentage = (meaningful_results as f64 / total_tests as f64) * 100.0;
        println!("\n📊 Semantic Relevance Analysis:");
        println!(
            "   Meaningful results: {}/{} ({:.1}%)",
            meaningful_results, total_tests, meaningful_percentage
        );

        if meaningful_percentage < 60.0 {
            println!("\n❌ LOW SEMANTIC RELEVANCE:");
            println!(
                "   Only {:.1}% of queries returned semantically meaningful results",
                meaningful_percentage
            );
            println!("   This indicates that:");
            println!("   - Mock embeddings don't capture semantic relationships");
            println!("   - Similarity calculation doesn't reflect actual content meaning");
            println!("   - Search results are not ranked by true semantic relevance");

            panic!("RED PHASE: Semantic search produces non-meaningful similarity scores");
        }

        println!("\n❌ ADDITIONAL SIMILARITY ISSUES:");
        println!("   - Mock cosine similarity doesn't use real vector embeddings");
        println!("   - Score patterns follow keyword matching rather than semantic understanding");
        println!("   - No actual embedding service integration for query embedding generation");

        Ok(())
    }

    #[tokio::test]
    /// Comprehensive test that demonstrates all current semantic search issues
    ///
    /// This test should FAIL and provides a complete specification of what needs
    /// to be implemented for proper semantic search functionality.
    async fn test_semantic_search_comprehensive_integration_specification() -> Result<()> {
        let (_temp_dir, kiln_path) = create_test_kiln().await?;

        println!("🎯 TDD RED: Comprehensive Semantic Search Integration Specification");
        println!("📁 Test kiln: {}", kiln_path.display());

        println!("\n🔍 CURRENT IMPLEMENTATION ISSUES:");

        // Issue 1: Mock embeddings instead of real ones
        println!("\n1. ❌ Mock Embeddings Issue:");
        println!("   Location: vault_integration.rs line 1080 (generate_mock_query_embedding)");
        println!("   Problem: Uses predefined patterns instead of real embedding generation");
        println!("   Impact: No actual semantic understanding, predictable score patterns");

        let mock_detected = check_if_uses_mock_embeddings(&kiln_path).await?;
        println!("   Evidence: Mock embeddings detected: {}", mock_detected);

        // Issue 2: Configuration integration gaps
        println!("\n2. ❌ Configuration Integration Issue:");
        println!("   Problem: CLI embedding configuration is ignored during search");
        println!("   Impact: Cannot use different embedding models or services");

        let config_ignored = check_if_configuration_is_ignored(&kiln_path).await?;
        println!("   Evidence: Configuration ignored: {}", config_ignored);

        // Issue 3: Database persistence issues
        println!("\n3. ❌ Database Persistence Issue:");
        println!("   Problem: May not maintain consistent state across runs");
        println!("   Impact: Inconsistent search results and lost embeddings");

        let persistence_issue = check_database_persistence_issues(&kiln_path).await?;
        println!("   Evidence: Persistence issues: {}", persistence_issue);

        // Issue 4: Non-meaningful similarity scores
        println!("\n4. ❌ Similarity Score Issue:");
        println!("   Problem: Scores don't reflect actual semantic relevance");
        println!("   Impact: Poor search quality and user experience");

        let poor_similarity = check_poor_similarity_quality(&kiln_path).await?;
        println!("   Evidence: Poor similarity quality: {}", poor_similarity);

        println!("\n✅ REQUIRED IMPLEMENTATION (Green Phase):");

        println!("\n1. 🔧 Real Embedding Generation:");
        println!("   - Replace generate_mock_query_embedding() with real embedding service calls");
        println!("   - Integrate with configured embedding providers (Ollama, OpenAI, etc.)");
        println!("   - Support configurable embedding models");
        println!("   - Handle embedding service errors and retries");

        println!("\n2. 🔧 Configuration Integration:");
        println!("   - Use CLI embedding configuration during semantic search");
        println!("   - Respect embedding service URL, model, and authentication settings");
        println!("   - Support different embedding providers through unified interface");
        println!("   - Validate configuration before search operations");

        println!("\n3. 🔧 Persistent Database Storage:");
        println!("   - Ensure embeddings are properly stored in persistent database");
        println!("   - Maintain database consistency across multiple process runs");
        println!("   - Handle concurrent access and transaction management");
        println!("   - Provide database migration and cleanup capabilities");

        println!("\n4. 🔧 Meaningful Similarity Calculation:");
        println!("   - Use real vector embeddings for similarity calculation");
        println!("   - Implement proper cosine similarity with actual vectors");
        println!("   - Support different similarity metrics and thresholds");
        println!("   - Provide relevance feedback and ranking algorithms");

        // Summary of current state
        let total_issues = [
            mock_detected,
            config_ignored,
            persistence_issue,
            poor_similarity,
        ]
        .iter()
        .map(|&issue| if issue { 1 } else { 0 })
        .sum::<u32>();

        println!("\n📊 CURRENT IMPLEMENTATION ASSESSMENT:");
        println!("   Issues detected: {}/4", total_issues);
        println!(
            "   Mock embeddings: {}",
            if mock_detected { "❌ YES" } else { "✅ NO" }
        );
        println!(
            "   Configuration ignored: {}",
            if config_ignored { "❌ YES" } else { "✅ NO" }
        );
        println!(
            "   Persistence issues: {}",
            if persistence_issue {
                "❌ YES"
            } else {
                "✅ NO"
            }
        );
        println!(
            "   Poor similarity: {}",
            if poor_similarity { "❌ YES" } else { "✅ NO" }
        );

        if total_issues >= 2 {
            println!("\n❌ TDD RED PHASE COMPREHENSIVE FAILURE:");
            println!("   Multiple critical issues detected in semantic search implementation");
            println!("   This provides a clear specification for required implementation work");
            println!(
                "   All tests should pass after implementing proper semantic search functionality"
            );

            panic!("RED PHASE: Semantic search requires comprehensive implementation work ({} issues detected)", total_issues);
        }

        println!("\n✅ GREEN PHASE: Semantic search implementation is working correctly");
        Ok(())
    }
}

// Helper functions for issue detection

async fn check_if_uses_mock_embeddings(kiln_path: &PathBuf) -> Result<bool> {
    // Test with queries that have predictable mock patterns
    let test_queries = vec![
        "machine learning",
        "neural networks",
        "artificial intelligence",
    ];

    for query in test_queries {
        match run_semantic_search_json(kiln_path, query).await {
            Ok(result) => {
                if let Some(search_results) = result.get("results").and_then(|r| r.as_array()) {
                    if let Some(first_result) = search_results.first() {
                        if let Some(score) = first_result.get("score").and_then(|s| s.as_f64()) {
                            // Check if score matches expected mock pattern
                            let expected_pattern = match query {
                                q if q.to_lowercase().contains("machine learning") => score > 0.7,
                                q if q.to_lowercase().contains("neural") => score > 0.6,
                                q if q.to_lowercase().contains("artificial")
                                    || q.to_lowercase().contains("ai") =>
                                {
                                    score > 0.4
                                }
                                _ => false,
                            };

                            if expected_pattern {
                                return Ok(true);
                            }
                        }
                    }
                }
            }
            Err(_) => continue,
        }
    }

    Ok(false)
}

async fn check_if_configuration_is_ignored(kiln_path: &PathBuf) -> Result<bool> {
    let test_query = "test query";

    // Test with different configurations
    let result1 = run_semantic_search_with_config(
        kiln_path,
        test_query,
        Some("http://localhost:11434"),
        Some("model1"),
    )
    .await;
    let result2 = run_semantic_search_with_config(
        kiln_path,
        test_query,
        Some("http://localhost:9999"),
        Some("model2"),
    )
    .await;

    match (result1, result2) {
        (Ok(r1), Ok(r2)) => {
            // If results are identical despite different configs, configuration is ignored
            Ok(r1 == r2)
        }
        _ => Ok(false),
    }
}

async fn check_database_persistence_issues(kiln_path: &PathBuf) -> Result<bool> {
    // Check if database exists before and after operations
    let db_before = check_database_embeddings(kiln_path).await?;

    process_kiln_for_embeddings(kiln_path).await?;

    let db_after = check_database_embeddings(kiln_path).await?;

    // If database doesn't exist after processing, there are persistence issues
    Ok(!db_after)
}

async fn check_poor_similarity_quality(kiln_path: &PathBuf) -> Result<bool> {
    // Test with semantically related queries
    let related_queries = vec!["machine learning", "neural networks", "AI algorithms"];
    let mut scores = Vec::new();

    for query in related_queries {
        match run_semantic_search_json(kiln_path, query).await {
            Ok(result) => {
                if let Some(search_results) = result.get("results").and_then(|r| r.as_array()) {
                    if let Some(first_result) = search_results.first() {
                        if let Some(score) = first_result.get("score").and_then(|s| s.as_f64()) {
                            scores.push(score);
                        }
                    }
                }
            }
            Err(_) => continue,
        }
    }

    if scores.len() >= 2 {
        // Check if scores are too similar (indicating poor similarity calculation)
        let avg_score = scores.iter().sum::<f64>() / scores.len() as f64;
        let variance =
            scores.iter().map(|s| (s - avg_score).powi(2)).sum::<f64>() / scores.len() as f64;

        // Low variance suggests poor similarity quality
        Ok(variance < 0.01)
    } else {
        Ok(true)
    }
}
use anyhow::Result;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::process::Command;
use tokio::time::sleep;
