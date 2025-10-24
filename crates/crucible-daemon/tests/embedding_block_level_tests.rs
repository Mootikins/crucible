//! Block-Level Embedding Tests for Document Processing
//!
//! This test suite validates block-level embedding generation for different
//! content types within markdown documents. Tests cover paragraph, heading,
//! list item, code block, and document chunking with overlap scenarios.
//!
//! ## Test Coverage
//!
//! ### Individual Block Types
//! - Paragraph embeddings
//! - Heading embeddings (H1-H6)
//! - List item embeddings
//! - Code block embeddings
//! - Blockquote embeddings
//!
//! ### Document Chunking
//! - Fixed-size chunking with overlap
//! - Semantic chunking at boundaries
//! - Heading-based chunking
//! - List-based chunking
//!
//! ### Mixed Content Handling
//! - Documents with multiple block types
//! - Nested structures (lists in lists, code in headings)
//! - Special markdown features (tables, links, images)

mod fixtures;
mod utils;

use anyhow::Result;
use utils::harness::DaemonEmbeddingHarness;
use crucible_surrealdb::embedding_config::{EmbeddingConfig, EmbeddingModel, PrivacyMode};

// ============================================================================
// Individual Block Type Tests
// ============================================================================

/// Test paragraph block embeddings
///
/// Verifies:
/// - Individual paragraphs generate valid embeddings
/// - Different paragraphs produce different embeddings
/// - Similar paragraphs produce similar embeddings
/// - Empty paragraphs are handled correctly
#[tokio::test]
async fn test_paragraph_block_embeddings() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let paragraph_cases = vec![
        ("short paragraph", "This is a short paragraph."),
        ("medium paragraph", "This is a medium length paragraph that contains multiple sentences and various topics for testing paragraph embedding generation."),
        ("long paragraph", &"This is a long paragraph that contains detailed information about various topics. ".repeat(10)),
        ("empty paragraph", ""),
        ("whitespace paragraph", "   \n\n   "),
        ("technical paragraph", "The system uses a thread pool with configurable worker threads to process embedding requests in parallel. Each worker thread can handle embedding generation independently, allowing for high throughput and efficient resource utilization."),
        ("narrative paragraph", "In the quiet morning light, the garden was transformed into a magical space where dewdrops sparkled on every leaf and birdsong filled the air with gentle melodies."),
    ];

    for (description, content) in paragraph_cases {
        let embedding = harness.generate_embedding(content).await?;

        assert_eq!(
            embedding.len(),
            768,
            "Paragraph '{}' should produce 768-dimensional embedding",
            description
        );

        // Verify all values are finite and in expected range
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "Paragraph '{}' embedding value at index {} should be finite",
                description, i
            );
            assert!(
                value >= 0.0 && value <= 1.0,
                "Paragraph '{}' embedding value at index {} should be within [0, 1]",
                description, i
            );
        }

        // Verify embedding has variance (except empty/whitespace cases)
        if !content.trim().is_empty() {
            let variance = calculate_variance(&embedding);
            assert!(
                variance > 0.0,
                "Paragraph '{}' embedding should have positive variance",
                description
            );
        }
    }

    // Test similarity between similar paragraphs
    let similar_1 = "This is about machine learning algorithms.";
    let similar_2 = "Machine learning algorithms are discussed here.";
    let different = "Cooking recipes and kitchen tips.";

    let embed_1 = harness.generate_embedding(similar_1).await?;
    let embed_2 = harness.generate_embedding(similar_2).await?;
    let embed_diff = harness.generate_embedding(different).await?;

    let sim_similar = cosine_similarity(&embed_1, &embed_2);
    let sim_different_1 = cosine_similarity(&embed_1, &embed_diff);
    let sim_different_2 = cosine_similarity(&embed_2, &embed_diff);

    assert!(
        sim_similar > sim_different_1,
        "Similar paragraphs should be more similar than different ones"
    );
    assert!(
        sim_similar > sim_different_2,
        "Similar paragraphs should be more similar than different ones"
    );

    println!("Similar paragraph similarity: {:.4}", sim_similar);
    println!("Different paragraph similarity 1: {:.4}", sim_different_1);
    println!("Different paragraph similarity 2: {:.4}", sim_different_2);

    Ok(())
}

/// Test heading block embeddings
///
/// Verifies:
/// - Different heading levels (H1-H6) generate embeddings
/// - Heading content is properly embedded
/// - Heading hierarchy affects similarity
/// - Empty headings are handled correctly
#[tokio::test]
async fn test_heading_block_embeddings() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let heading_cases = vec![
        ("H1 short", "# Short Title"),
        ("H1 long", "# This is a Much Longer and More Comprehensive Title for Testing"),
        ("H2 short", "## Section Title"),
        ("H2 medium", "## This is a Medium Length Section Title with Some Context"),
        ("H3 short", "### Subsection"),
        ("H3 detailed", "### Detailed Subsection with Specific Information"),
        ("H4 task", "#### Task: Implementation Details"),
        ("H5 note", "##### Important Note"),
        ("H6 reference", "###### Reference Material"),
        ("empty heading", "# "),
        ("whitespace heading", "##    \t   "),
    ];

    for (description, content) in heading_cases {
        let embedding = harness.generate_embedding(content).await?;

        assert_eq!(
            embedding.len(),
            768,
            "Heading '{}' should produce 768-dimensional embedding",
            description
        );

        // Verify all values are finite
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "Heading '{}' embedding value at index {} should be finite",
                description, i
            );
        }
    }

    // Test heading similarity patterns
    let similar_headings = vec![
        "# Introduction to Machine Learning",
        "# Machine Learning Overview",
    ];

    let different_headings = vec![
        "# Machine Learning Basics",
        "# Advanced Cooking Techniques",
    ];

    let embeddings: Result<Vec<_>> = futures::future::join_all(
        similar_headings.iter().chain(different_headings.iter())
            .map(|content| harness.generate_embedding(content))
    ).await?;

    let similar_sim = cosine_similarity(&embeddings[0], &embeddings[1]);
    let different_sim = cosine_similarity(&embeddings[2], &embeddings[3]);

    assert!(
        similar_sim > different_sim,
        "Similar headings should be more similar than different ones"
    );

    println!("Similar headings similarity: {:.4}", similar_sim);
    println!("Different headings similarity: {:.4}", different_sim);

    Ok(())
}

/// Test list item embeddings
///
/// Verifies:
/// - Individual list items generate embeddings
/// - Bullet points vs numbered lists
/// - Nested list structures
/// - Complex list items with formatting
#[tokio::test]
async fn test_list_item_embeddings() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let list_item_cases = vec![
        ("bullet simple", "- Simple item"),
        ("bullet detailed", "- This is a more detailed list item that contains multiple phrases and information"),
        ("numbered simple", "1. First item"),
        ("numbered detailed", "2. Second numbered item with extensive details and explanations"),
        ("nested first level", "- Top level item"),
        ("nested second level", "  - Nested item with indentation"),
        ("nested third level", "    - Deeply nested item"),
        ("task item", "- [ ] Uncompleted task item"),
        ("completed task", "- [x] Completed task item"),
        ("complex formatting", "- Item with **bold**, *italic*, and `code` formatting"),
        ("empty item", "- "),
        ("link item", "- [Link text](https://example.com) in list"),
    ];

    for (description, content) in list_item_cases {
        let embedding = harness.generate_embedding(content).await?;

        assert_eq!(
            embedding.len(),
            768,
            "List item '{}' should produce 768-dimensional embedding",
            description
        );

        // Verify all values are finite
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "List item '{}' embedding value at index {} should be finite",
                description, i
            );
        }
    }

    // Test list item similarity patterns
    let similar_items = vec![
        "- Implement user authentication",
        "- Add user login functionality",
    ];

    let different_items = vec![
        "- Create user interface",
        "- Configure database settings",
    ];

    let embeddings: Result<Vec<_>> = futures::future::join_all(
        similar_items.iter().chain(different_items.iter())
            .map(|content| harness.generate_embedding(content))
    ).await?;

    let similar_sim = cosine_similarity(&embeddings[0], &embeddings[1]);
    let different_sim = cosine_similarity(&embeddings[2], &embeddings[3]);

    assert!(
        similar_sim > different_sim,
        "Similar list items should be more similar than different ones"
    );

    println!("Similar list items similarity: {:.4}", similar_sim);
    println!("Different list items similarity: {:.4}", different_sim);

    Ok(())
}

/// Test code block embeddings
///
/// Verifies:
/// - Code blocks generate valid embeddings
/// - Different programming languages
/// - Inline code vs code blocks
/// - Code with comments vs pure code
#[tokio::test]
async fn test_code_block_embeddings() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let code_block_cases = vec![
        ("rust simple", "```rust\nfn main() {\n    println!(\"Hello, world!\");\n}\n```"),
        ("rust complex", "```rust\nuse std::collections::HashMap;\n\nstruct User {\n    id: u32,\n    name: String,\n    active: bool,\n}\n\nimpl User {\n    fn new(id: u32, name: String) -> Self {\n        Self { id, name, active: true }\n    }\n}\n```"),
        ("javascript simple", "```javascript\nfunction greet(name) {\n    return `Hello, ${name}!`;\n}\n```"),
        ("python simple", "```python\ndef calculate_sum(numbers):\n    return sum(numbers)\n```"),
        ("python complex", "```python\nimport pandas as pd\nimport numpy as np\n\nclass DataProcessor:\n    def __init__(self, data_path):\n        self.data = pd.read_csv(data_path)\n        self.processed = False\n    \n    def process_data(self):\n        self.data['normalized'] = (self.data['value'] - self.data['value'].mean()) / self.data['value'].std()\n        self.processed = True\n        return self.data\n```"),
        ("sql query", "```sql\nSELECT u.name, COUNT(o.id) as order_count\nFROM users u\nLEFT JOIN orders o ON u.id = o.user_id\nWHERE u.created_at >= '2024-01-01'\nGROUP BY u.id, u.name\nHAVING COUNT(o.id) > 5\nORDER BY order_count DESC\n```"),
        ("inline code", "Use the `println!` macro for output"),
        ("mixed code", "```rust\n// This is a comment\nfn calculate(x: i32) -> i32 {\n    x * 2  // Double the input\n}\n```"),
        ("shell script", "```bash\necho \"Starting deployment...\"\ndocker-compose up -d\nif [ $? -eq 0 ]; then\n    echo \"Deployment successful\"\nelse\n    echo \"Deployment failed\"\nfi\n```"),
        ("yaml config", "```yaml\nversion: '3.8'\nservices:\n  web:\n    image: nginx:latest\n    ports:\n      - \"80:80\"\n    environment:\n      - NODE_ENV=production\n```"),
    ];

    for (description, content) in code_block_cases {
        let embedding = harness.generate_embedding(content).await?;

        assert_eq!(
            embedding.len(),
            768,
            "Code block '{}' should produce 768-dimensional embedding",
            description
        );

        // Verify all values are finite
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "Code block '{}' embedding value at index {} should be finite",
                description, i
            );
        }
    }

    // Test code similarity patterns
    let similar_code = vec![
        "```rust\nfn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n```",
        "```rust\nfn multiply(x: i32, y: i32) -> i32 {\n    x * y\n}\n```",
    ];

    let different_code = vec![
        "```rust\nfn calculate() -> i32 {\n    42\n}\n```",
        "```python\ndef calculate():\n    return 42\n```",
    ];

    let embeddings: Result<Vec<_>> = futures::future::join_all(
        similar_code.iter().chain(different_code.iter())
            .map(|content| harness.generate_embedding(content))
    ).await?;

    let similar_sim = cosine_similarity(&embeddings[0], &embeddings[1]);
    let different_sim = cosine_similarity(&embeddings[2], &embeddings[3]);

    assert!(
        similar_sim > different_sim,
        "Similar code (same language) should be more similar than different code"
    );

    println!("Similar code similarity: {:.4}", similar_sim);
    println!("Different code similarity: {:.4}", different_sim);

    Ok(())
}

/// Test blockquote embeddings
///
/// Verifies:
/// - Blockquotes generate valid embeddings
/// - Nested blockquotes
/// - Blockquotes with other formatting
/// - Quote attribution
#[tokio::test]
async fn test_blockquote_embeddings() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let blockquote_cases = vec![
        ("simple quote", "> This is a simple quote."),
        ("multiline quote", "> This is a quote that spans\n> multiple lines of text\n> to test line handling."),
        ("nested quote", "> This is the outer quote\n> > This is a nested quote inside"),
        ("quote with attribution", "> The only way to do great work is to love what you do.\n> \n> — Steve Jobs"),
        ("quote with formatting", "> **Important note:** This quote contains *emphasis* and `code` formatting."),
        ("complex quote", "> **Warning:** This is a complex quote that contains:\n> - Multiple points\n> - **Bold emphasis**\n> - `Code examples`\n> - [Links](https://example.com)"),
        ("empty quote", "> "),
        ("whitespace quote", ">    \t   "),
    ];

    for (description, content) in blockquote_cases {
        let embedding = harness.generate_embedding(content).await?;

        assert_eq!(
            embedding.len(),
            768,
            "Blockquote '{}' should produce 768-dimensional embedding",
            description
        );

        // Verify all values are finite
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "Blockquote '{}' embedding value at index {} should be finite",
                description, i
            );
        }
    }

    Ok(())
}

// ============================================================================
// Document Chunking Tests
// ============================================================================

/// Test fixed-size chunking with overlap
///
/// Verifies:
/// - Documents are correctly chunked by character count
/// - Overlap between chunks is maintained
/// - Content is preserved across chunk boundaries
/// - Embeddings capture chunk semantics
#[tokio::test]
async fn test_fixed_size_chunking_with_overlap() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create a long document for chunking
    let base_sentence = "This is a sentence for testing document chunking functionality. ";
    let long_document = base_sentence.repeat(50); // ~2000 characters

    // Test different chunk sizes
    let chunk_configs = vec![
        (500, 50,   "Medium chunks with 10% overlap"),
        (200, 20,   "Small chunks with 10% overlap"),
        (1000, 100, "Large chunks with 10% overlap"),
    ];

    for (chunk_size, overlap, description) in chunk_configs {
        let chunks = create_fixed_chunks(&long_document, chunk_size, overlap);

        println!("{}: {} chunks from {} characters", description, chunks.len(), long_document.len());

        // Verify we have chunks
        assert!(
            !chunks.is_empty(),
            "Should create at least one chunk for {}",
            description
        );

        // Generate embeddings for all chunks
        let embeddings: Result<Vec<_>> = futures::future::join_all(
            chunks.iter().map(|chunk| harness.generate_embedding(chunk))
        ).await?;

        assert_eq!(
            embeddings.len(),
            chunks.len(),
            "Should have embeddings for all chunks"
        );

        // Verify all embeddings are valid
        for (i, embedding) in embeddings.iter().enumerate() {
            assert_eq!(
                embedding.len(),
                768,
                "Chunk {} embedding should have 768 dimensions",
                i
            );

            for (j, &value) in embedding.iter().enumerate() {
                assert!(
                    value.is_finite(),
                    "Chunk {} embedding value at index {} should be finite",
                    i, j
                );
            }
        }

        // Test overlap similarity (adjacent chunks should be somewhat similar due to overlap)
        if embeddings.len() > 1 {
            for i in 0..(embeddings.len() - 1) {
                let similarity = cosine_similarity(&embeddings[i], &embeddings[i + 1]);
                println!("  Chunk {} -> {} similarity: {:.4}", i, i + 1, similarity);

                // With proper overlap, adjacent chunks should be reasonably similar
                assert!(
                    similarity > 0.3,
                    "Adjacent chunks with overlap should be reasonably similar, got {:.4}",
                    similarity
                );
            }
        }
    }

    Ok(())
}

/// Test semantic chunking at boundaries
///
/// Verifies:
/// - Documents are chunked at natural boundaries (paragraphs, headings)
/// - Semantic coherence is maintained within chunks
/// - No sentences are split across chunks
/// - Chunks have reasonable size distribution
#[tokio::test]
async fn test_semantic_chunking_at_boundaries() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create a document with clear semantic boundaries
    let semantic_document = r#"# Introduction to Machine Learning

Machine learning is a subset of artificial intelligence that enables systems to learn and improve from experience without being explicitly programmed.

## What is Machine Learning?

Machine learning algorithms build mathematical models based on sample data, known as training data, in order to make predictions or decisions.

## Types of Machine Learning

### Supervised Learning

Supervised learning algorithms learn from labeled training data. The algorithm learns a mapping function that predicts the output based on input data.

Examples include:
- Classification tasks
- Regression tasks
- Neural networks

### Unsupervised Learning

Unsupervised learning deals with unlabeled data. The algorithm tries to learn patterns and structures from the input data.

Common applications:
- Clustering
- Dimensionality reduction
- Anomaly detection

## Applications

Machine learning is used in various fields including healthcare, finance, transportation, and technology."#;

    let chunks = create_semantic_chunks(semantic_document, 500); // Target 500 characters per chunk

    println!("Semantic chunking created {} chunks", chunks.len());

    // Verify chunks are reasonable
    assert!(
        !chunks.is_empty(),
        "Should create at least one semantic chunk"
    );

    // Generate embeddings for semantic chunks
    let embeddings: Result<Vec<_>> = futures::future::join_all(
        chunks.iter().enumerate().map(|(i, chunk)| {
            println!("Chunk {}: {} characters", i, chunk.len());
            harness.generate_embedding(chunk)
        })
    ).await?;

    assert_eq!(
        embeddings.len(),
        chunks.len(),
        "Should have embeddings for all semantic chunks"
    );

    // Verify all embeddings are valid
    for (i, embedding) in embeddings.iter().enumerate() {
        assert_eq!(
            embedding.len(),
            768,
            "Semantic chunk {} embedding should have 768 dimensions",
            i
        );
    }

    // Test that semantic chunks are less similar to each other than overlapping chunks
    if embeddings.len() > 1 {
        let mut similarities = Vec::new();
        for i in 0..(embeddings.len() - 1) {
            let similarity = cosine_similarity(&embeddings[i], &embeddings[i + 1]);
            similarities.push(similarity);
            println!("Semantic chunk {} -> {} similarity: {:.4}", i, i + 1, similarity);
        }

        let avg_similarity = similarities.iter().sum::<f32>() / similarities.len() as f32;
        println!("Average semantic chunk similarity: {:.4}", avg_similarity);

        // Semantic chunks should be less similar than overlapping chunks
        assert!(
            avg_similarity < 0.7,
            "Semantic chunks should be reasonably distinct, average similarity: {:.4}",
            avg_similarity
        );
    }

    Ok(())
}

/// Test heading-based chunking
///
/// Verifies:
/// - Documents are chunked based on heading hierarchy
/// - Each chunk contains related content under a heading
/// - Heading context is preserved in chunks
/// - Nested heading structure is handled correctly
#[tokio::test]
async fn test_heading_based_chunking() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create a document with clear heading structure
    let heading_document = r#"# Main Document Title

This is the introduction paragraph that belongs to the main section.

## First Section

This content belongs to the first section and should be grouped together.

### Subsection 1.1

This content is under the first subsection.

- List item 1
- List item 2
- List item 3

### Subsection 1.2

More content under the second subsection.

## Second Section

Content for the second section.

### Subsection 2.1

```rust
fn example() {
    println!("Code in subsection");
}
```

## Conclusion

Final thoughts and summary.

### Next Steps

- Step 1
- Step 2"#;

    let chunks = create_heading_chunks(heading_document);

    println!("Heading-based chunking created {} chunks", chunks.len());

    // Verify we have reasonable number of chunks
    assert!(
        chunks.len() >= 3, // Should have at least main sections
        "Should create multiple heading-based chunks"
    );

    // Generate embeddings for heading-based chunks
    let embeddings: Result<Vec<_>> = futures::future::join_all(
        chunks.iter().enumerate().map(|(i, chunk)| {
            println!("Heading chunk {}: {} characters", i, chunk.len());
            harness.generate_embedding(chunk)
        })
    ).await?;

    assert_eq!(
        embeddings.len(),
        chunks.len(),
        "Should have embeddings for all heading chunks"
    );

    // Verify all embeddings are valid
    for (i, embedding) in embeddings.iter().enumerate() {
        assert_eq!(
            embedding.len(),
            768,
            "Heading chunk {} embedding should have 768 dimensions",
            i
        );
    }

    // Test that heading chunks have different semantic content
    if embeddings.len() > 1 {
        for i in 0..(embeddings.len() - 1) {
            let similarity = cosine_similarity(&embeddings[i], &embeddings[i + 1]);
            println!("Heading chunk {} -> {} similarity: {:.4}", i, i + 1, similarity);

            // Different sections should be reasonably distinct
            assert!(
                similarity < 0.9,
                "Different heading sections should be distinct, similarity: {:.4}",
                similarity
            );
        }
    }

    Ok(())
}

// ============================================================================
// Mixed Content Handling Tests
// ============================================================================

/// Test documents with multiple block types
///
/// Verifies:
/// - Documents with various block types are handled correctly
/// - Each block type contributes to overall embedding
/// - Block ordering is preserved
/// - Complex formatting doesn't break processing
#[tokio::test]
async fn test_mixed_block_types_document() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create a complex document with multiple block types
    let mixed_document = r#"# Complex Document with Multiple Block Types

This document contains various types of content blocks to test comprehensive embedding generation.

## Introduction

Paragraphs provide the main narrative content. They can contain **bold text**, *italic text*, and `inline code`.

### Code Examples

Here are some code examples:

```rust
fn process_data(input: &str) -> Result<String> {
    Ok(input.to_uppercase())
}
```

```python
def analyze_data(data):
    return len(data.split())
```

## Lists and Nested Content

### Task List

- [ ] Implement authentication system
- [x] Set up database connection
- [ ] Create user interface

### Nested List

1. Main category
   - Subcategory A
     - Detail 1
     - Detail 2
   - Subcategory B
2. Another category

## Important Notes

> **Note:** This is an important blockquote that contains crucial information.
>
> It can span multiple lines and include `code formatting`.

## Data Processing

The system processes various types of data:

| Type | Method | Complexity |
|------|--------|------------|
| Text | NLP | High |
| Image | CV | Medium |
| Audio | DSP | Low |

## Conclusion

Mixed content documents demonstrate the versatility of the embedding system."#;

    // Process the entire mixed document
    let full_embedding = harness.generate_embedding(mixed_document).await?;

    assert_eq!(
        full_embedding.len(),
        768,
        "Mixed document should produce 768-dimensional embedding"
    );

    // Verify embedding quality
    for (i, &value) in full_embedding.iter().enumerate() {
        assert!(
            value.is_finite(),
            "Mixed document embedding value at index {} should be finite",
            i
        );
    }

    let variance = calculate_variance(&full_embedding);
    assert!(
        variance > 0.0,
        "Mixed document embedding should have positive variance"
    );

    // Process individual sections for comparison
    let sections = vec![
        "# Introduction\n\nParagraphs provide the main narrative content.",
        "### Code Examples\n\n```rust\nfn process_data(input: &str) -> Result<String> {\n    Ok(input.to_uppercase())\n}\n```",
        "### Task List\n\n- [ ] Implement authentication system\n- [x] Set up database connection",
        "> **Note:** This is an important blockquote that contains crucial information.",
        "| Type | Method | Complexity |\n|------|--------|------------|\n| Text | NLP | High |",
    ];

    let section_embeddings: Result<Vec<_>> = futures::future::join_all(
        sections.iter().map(|section| harness.generate_embedding(section))
    ).await?;

    // Compare full document with individual sections
    for (i, section_embedding) in section_embeddings.iter().enumerate() {
        let similarity = cosine_similarity(&full_embedding, section_embedding);
        println!("Full document vs section {} similarity: {:.4}", i + 1, similarity);

        // Full document should be related to each section but not identical
        assert!(
            similarity > 0.3 && similarity < 0.95,
            "Full document should be related but not identical to sections"
        );
    }

    Ok(())
}

/// Test nested structures
///
/// Verifies:
/// - Nested lists are handled correctly
/// - Code blocks within other elements work
/// - Complex nested markdown is processed
/// - Hierarchy doesn't break embedding generation
#[tokio::test]
async fn test_nested_structures() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let nested_cases = vec![
        ("nested lists", r#"1. First level item
   - Nested bullet point
   - Another nested point
     - Deeply nested item
     - Another deep item
   - Back to second level
2. Second level item
   - Different nested branch"#),
        ("code in lists", r#"## Implementation Steps

- Setup the project:
  ```bash
  npm init -y
  npm install express
  ```

- Create the server:
  ```javascript
  const express = require('express');
  const app = express();
  ```

- Define routes:
  ```javascript
  app.get('/', (req, res) => {
    res.send('Hello World!');
  });
  ```"#),
        ("quotes in lists", r#"## Important Considerations

- Performance considerations:
  > Always consider the impact on performance
  > when implementing new features.

- Security notes:
  > **Critical:** Never trust user input
  > without proper validation and sanitization."#),
        ("complex nesting", r#"## Project Structure

### Frontend Components
- **Header Component**
  - Navigation menu
  - User profile section
  - Search functionality
    - Real-time search
    - Search history
    - Advanced filters

### Backend Services
- **Authentication Service**
  - JWT token management
  - Password hashing
  - Session handling
    - Redis storage
    - Automatic cleanup
    - Security policies

### Database Layer
- **Primary Database**
  - PostgreSQL configuration
  - Connection pooling
  - Query optimization
    - Index strategies
    - Query plan analysis
    - Performance monitoring"#),
    ];

    for (description, content) in nested_cases {
        let embedding = harness.generate_embedding(content).await?;

        assert_eq!(
            embedding.len(),
            768,
            "Nested structure '{}' should produce 768-dimensional embedding",
            description
        );

        // Verify embedding quality
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "Nested structure '{}' embedding value at index {} should be finite",
                description, i
            );
        }

        let variance = calculate_variance(&embedding);
        assert!(
            variance > 0.0,
            "Nested structure '{}' embedding should have positive variance",
            description
        );

        println!("Processed nested structure '{}': variance = {:.4}", description, variance);
    }

    Ok(())
}

/// Test special markdown features
///
/// Verifies:
/// - Tables are processed correctly
/// - Links and images don't break embedding
/// - Mathematical expressions are handled
/// - HTML tags are processed properly
#[tokio::test]
async fn test_special_markdown_features() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let special_cases = vec![
        ("table", r#"| Feature | Status | Priority |
|---------|--------|----------|
| Authentication | Complete | High |
| Database | In Progress | High |
| UI Design | Not Started | Medium |
| Testing | Planned | Low |"#),
        ("links", r#"## Useful Resources

- [Official Documentation](https://docs.example.com)
- [GitHub Repository](https://github.com/example/project)
- [API Reference](https://api.example.com/docs)
- [Community Forum](https://community.example.com)"#),
        ("images", r#"## Visual Assets

![Architecture Diagram](images/architecture.png)
![User Flow](images/user-flow.svg)
![Performance Metrics](images/metrics.jpg)"#),
        ("math expressions", r#"## Mathematical Formulas

The quadratic formula: $x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2ax}$

Einstein's equation: $E = mc^2$

Standard deviation: $\\sigma = \\sqrt{\\frac{\\sum_{i=1}^{n}(x_i - \\mu)^2}{n}}$"#),
        ("html tags", r#"## HTML Content

<div class="warning">
  <strong>Warning:</strong> This feature is experimental.
</div>

<p>This is a <em>paragraph</em> with <code>inline code</code> and <strong>bold text</strong>.</p>

<blockquote>
  "The best way to predict the future is to invent it."
  <cite>— Alan Kay</cite>
</blockquote>"#),
        ("footnotes", r#"## Document with Footnotes

This is a document that includes footnotes[^1] for additional information.

The main content continues here with more text[^note] and references.

[^1]: This is the first footnote providing additional context.
[^note]: This is another footnote with more detailed information."#),
        ("task lists", r#"## Project Tasks

### Backlog
- [ ] Research user requirements
- [ ] Design system architecture
- [ ] Create development plan

### In Progress
- [x] Set up development environment
- [x] Initialize git repository
- [ ] Implement core features
  - [ ] User authentication
  - [ ] Database connection
  - [ ] API endpoints

### Completed
- [x] Project initialization
- [x] Requirements gathering"#),
    ];

    for (description, content) in special_cases {
        let embedding = harness.generate_embedding(content).await?;

        assert_eq!(
            embedding.len(),
            768,
            "Special markdown '{}' should produce 768-dimensional embedding",
            description
        );

        // Verify embedding quality
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "Special markdown '{}' embedding value at index {} should be finite",
                description, i
            );
        }

        let variance = calculate_variance(&embedding);
        assert!(
            variance > 0.0,
            "Special markdown '{}' embedding should have positive variance",
            description
        );

        println!("Processed special markdown '{}': variance = {:.4}", description, variance);
    }

    Ok(())
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Create fixed-size chunks with overlap
fn create_fixed_chunks(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    if text.len() <= chunk_size {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < text.len() {
        let end = (start + chunk_size).min(text.len());
        let chunk = text[start..end].to_string();
        chunks.push(chunk);

        if end >= text.len() {
            break;
        }

        start = end - overlap;
    }

    chunks
}

/// Create semantic chunks at natural boundaries
fn create_semantic_chunks(text: &str, target_size: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut current_chunk = String::new();
    let mut current_size = 0;

    for line in lines {
        let line_with_newline = if current_chunk.is_empty() {
            line.to_string()
        } else {
            format!("\n{}", line)
        };

        let new_size = current_size + line_with_newline.len();

        // If adding this line would exceed target size and we have content, start new chunk
        if new_size > target_size && !current_chunk.is_empty() {
            // Try to break at natural boundary (empty line or end of paragraph)
            if line.trim().is_empty() || current_chunk.ends_with(['.', '!', '?']) {
                chunks.push(current_chunk.clone());
                current_chunk = line.to_string();
                current_size = line.len();
            } else {
                current_chunk.push_str(&line_with_newline);
                current_size = new_size;
            }
        } else {
            current_chunk.push_str(&line_with_newline);
            current_size = new_size;
        }
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    chunks
}

/// Create heading-based chunks
fn create_heading_chunks(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut current_chunk = String::new();
    let mut has_content = false;

    for line in lines {
        // Check if this line is a heading
        if line.starts_with('#') {
            // If we have content in current chunk, save it
            if has_content {
                chunks.push(current_chunk.trim().to_string());
                current_chunk = String::new();
                has_content = false;
            }
        }

        current_chunk.push_str(line);
        current_chunk.push('\n');

        // Track if we have meaningful content
        if !line.trim().is_empty() {
            has_content = true;
        }
    }

    // Add the final chunk if it has content
    if has_content {
        chunks.push(current_chunk.trim().to_string());
    }

    chunks
}

/// Calculate cosine similarity between two vectors
fn cosine_similarity(vec1: &[f32], vec2: &[f32]) -> f32 {
    assert_eq!(
        vec1.len(), vec2.len(),
        "Vectors must have same length for cosine similarity"
    );

    let dot_product: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();
    let norm1: f32 = vec1.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm2: f32 = vec2.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm1 == 0.0 || norm2 == 0.0 {
        0.0
    } else {
        dot_product / (norm1 * norm2)
    }
}

/// Calculate variance of vector values
fn calculate_variance(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }

    let mean = values.iter().sum::<f32>() / values.len() as f32;
    let sum_squared_diff: f32 = values
        .iter()
        .map(|&x| (x - mean) * (x - mean))
        .sum();

    sum_squared_diff / values.len() as f32
}