//! Content Type Handling Tests for Embedding System
//!
//! This test suite validates embedding generation for diverse content types
//! from the test kiln, including technical, academic, business, and multilingual content.
//!
//! ## Test Coverage
//!
//! ### Technical Content
//! - Code examples in multiple languages
//! - API documentation
//! - Configuration files
//! - Technical specifications
//!
//! ### Academic Content
//! - Research papers and citations
//! - Methodology sections
//! - Academic language and terminology
//! - Citation formats
//!
//! ### Business Content
//! - Meeting notes and action items
//! - Project management documents
//! - Budget and timeline information
//! - Business terminology
//!
//! ### Multilingual Content
//! - Unicode text in various scripts
//! - Mixed language documents
//! - Right-to-left text
//! - Special characters and symbols

mod fixtures;
mod utils;

use anyhow::Result;
use utils::harness::DaemonEmbeddingHarness;
use crucible_surrealdb::embedding_config::{EmbeddingConfig, EmbeddingModel, PrivacyMode};
use std::path::Path;

// ============================================================================
// Technical Content Tests
// ============================================================================

/// Test technical content with code examples
///
/// Verifies:
/// - Code in multiple programming languages is embedded correctly
/// - Technical terminology is captured
/// - Code comments and documentation are handled
/// - API endpoints and specifications are processed
#[tokio::test]
async fn test_technical_content_code_examples() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Read technical documentation from test kiln
    let tech_doc_path = "tests/test-kiln/Technical Documentation.md";
    let tech_content = std::fs::read_to_string(tech_doc_path)?;

    // Generate embedding for full technical document
    let tech_embedding = harness.generate_embedding(&tech_content).await?;

    assert_eq!(
        tech_embedding.len(),
        768,
        "Technical document should produce 768-dimensional embedding"
    );

    // Verify embedding quality
    for (i, &value) in tech_embedding.iter().enumerate() {
        assert!(
            value.is_finite(),
            "Technical document embedding value at index {} should be finite",
            i
        );
    }

    let variance = calculate_variance(&tech_embedding);
    assert!(
        variance > 0.0,
        "Technical document embedding should have positive variance"
    );

    println!("Technical document embedding variance: {:.4}", variance);

    // Test specific code snippets from the document
    let code_snippets = extract_code_snippets(&tech_content);

    for (i, snippet) in code_snippets.iter().enumerate() {
        let snippet_embedding = harness.generate_embedding(snippet).await?;

        assert_eq!(
            snippet_embedding.len(),
            768,
            "Code snippet {} should produce 768-dimensional embedding",
            i
        );

        // Compare snippet with full document
        let similarity = cosine_similarity(&tech_embedding, &snippet_embedding);
        println!("Code snippet {} similarity to full document: {:.4}", i + 1, similarity);

        // Code snippets should be related to the full document
        assert!(
            similarity > 0.2,
            "Code snippet should be related to full technical document"
        );
    }

    // Test different programming languages if present
    let languages = detect_programming_languages(&tech_content);
    println!("Detected programming languages: {:?}", languages);

    Ok(())
}

/// Test API documentation content
///
/// Verifies:
/// - API endpoints are processed correctly
/// - Request/response formats are handled
/// - Authentication and security information is captured
/// - Code examples in API docs work
#[tokio::test]
async fn test_api_documentation_content() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Read API documentation from test kiln
    let api_doc_path = "tests/test-kiln/API Documentation.md";
    let api_content = std::fs::read_to_string(api_doc_path)?;

    // Generate embedding for API documentation
    let api_embedding = harness.generate_embedding(&api_content).await?;

    assert_eq!(
        api_embedding.len(),
        768,
        "API documentation should produce 768-dimensional embedding"
    );

    // Extract and test specific API components
    let api_components = extract_api_components(&api_content);

    for (component_type, content) in api_components {
        let component_embedding = harness.generate_embedding(&content).await?;

        // Compare component with full API documentation
        let similarity = cosine_similarity(&api_embedding, &component_embedding);
        println!("API component '{}' similarity to full doc: {:.4}", component_type, similarity);

        // API components should be related to the full documentation
        assert!(
            similarity > 0.3,
            "API component '{}' should be related to full documentation",
            component_type
        );
    }

    // Test endpoint-specific content
    let endpoints = extract_api_endpoints(&api_content);
    println!("Found {} API endpoints", endpoints.len());

    for (i, endpoint) in endpoints.iter().enumerate() {
        if !endpoint.trim().is_empty() {
            let endpoint_embedding = harness.generate_embedding(endpoint).await?;
            assert_eq!(endpoint_embedding.len(), 768);

            let similarity = cosine_similarity(&api_embedding, &endpoint_embedding);
            println!("Endpoint {} similarity: {:.4}", i + 1, similarity);
        }
    }

    Ok(())
}

/// Test configuration and technical specifications
///
/// Verifies:
/// - Configuration files are processed
/// - Technical specifications are handled
/// - System architecture descriptions work
/// - Performance and security considerations are captured
#[tokio::test]
async fn test_configuration_and_specifications() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let config_cases = vec![
        ("yaml_config", r#"version: '3.8'
services:
  web:
    image: nginx:alpine
    ports:
      - "80:80"
      - "443:443"
    environment:
      - NODE_ENV=production
      - DEBUG=false
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
    restart: unless-stopped

  database:
    image: postgres:13
    environment:
      POSTGRES_DB: crucible
      POSTGRES_USER: crucible_user
      POSTGRES_PASSWORD: secure_password
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./init.sql:/docker-entrypoint-initdb.d/init.sql:ro
    restart: unless-stopped

volumes:
  postgres_data:"#),
        ("json_config", r#"{
  "server": {
    "host": "0.0.0.0",
    "port": 8080,
    "ssl": true,
    "cert_file": "/etc/ssl/certs/server.crt",
    "key_file": "/etc/ssl/private/server.key"
  },
  "database": {
    "type": "postgresql",
    "host": "localhost",
    "port": 5432,
    "name": "crucible",
    "pool_size": 20,
    "timeout": 30000
  },
  "auth": {
    "jwt_secret": "your-secret-key-here",
    "token_expiry": "24h",
    "refresh_token_expiry": "7d"
  },
  "logging": {
    "level": "info",
    "format": "json",
    "outputs": ["console", "file"]
  }
}"#),
        ("toml_config", r#"# Crucible Configuration

[app]
name = "Crucible Knowledge Management"
version = "2.1.0"
environment = "production"
debug = false

[server]
host = "127.0.0.1"
port = 3000
workers = 4
keep_alive = 30

[database]
url = "postgresql://user:pass@localhost/crucible"
max_connections = 100
connection_timeout = 30

[redis]
url = "redis://localhost:6379"
pool_size = 10

[security]
secret_key = "your-256-bit-secret"
session_timeout = 3600
max_login_attempts = 5
lockout_duration = 900

[features]
enable_embeddings = true
enable_collaboration = true
enable_analytics = false
max_file_size = "10MB""#),
    ];

    for (description, content) in config_cases {
        let embedding = harness.generate_embedding(content).await?;

        assert_eq!(
            embedding.len(),
            768,
            "Configuration '{}' should produce 768-dimensional embedding",
            description
        );

        // Verify embedding quality
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "Configuration '{}' embedding value at index {} should be finite",
                description, i
            );
        }

        let variance = calculate_variance(&embedding);
        assert!(
            variance > 0.0,
            "Configuration '{}' embedding should have positive variance",
            description
        );

        println!("Configuration '{}' embedding variance: {:.4}", description, variance);
    }

    Ok(())
}

// ============================================================================
// Academic Content Tests
// ============================================================================

/// Test academic research content
///
/// Verifies:
/// - Research papers are processed correctly
/// - Academic terminology is captured
/// - Citations and references are handled
/// - Methodology sections are understood
#[tokio::test]
async fn test_academic_research_content() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Read research methods from test kiln
    let research_doc_path = "tests/test-kiln/Research Methods.md";
    let research_content = std::fs::read_to_string(research_doc_path)?;

    // Generate embedding for academic content
    let research_embedding = harness.generate_embedding(&research_content).await?;

    assert_eq!(
        research_embedding.len(),
        768,
        "Research document should produce 768-dimensional embedding"
    );

    // Test academic sections
    let academic_sections = extract_academic_sections(&research_content);

    for (section_type, content) in academic_sections {
        let section_embedding = harness.generate_embedding(&content).await?;

        // Compare section with full research document
        let similarity = cosine_similarity(&research_embedding, &section_embedding);
        println!("Academic section '{}' similarity to full doc: {:.4}", section_type, similarity);

        // Academic sections should be related to the full document
        assert!(
            similarity > 0.3,
            "Academic section '{}' should be related to full document",
            section_type
        );
    }

    // Test specific academic terminology
    let academic_terms = extract_academic_terminology(&research_content);
    println!("Found {} academic terms", academic_terms.len());

    for term in academic_terms.iter().take(10) {
        let term_embedding = harness.generate_embedding(term).await?;
        assert_eq!(term_embedding.len(), 768);

        let similarity = cosine_similarity(&research_embedding, &term_embedding);
        println!("Academic term '{}' similarity: {:.4}", term, similarity);
    }

    Ok(())
}

/// Test citation and reference handling
///
/// Verifies:
/// - Different citation formats are handled
/// - Reference lists are processed
/// - DOI and URL links are handled
/// - Academic metadata is captured
#[tokio::test]
async fn test_citation_and_reference_handling() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let citation_cases = vec![
        ("apa_citation", "Smith, J. D. (2023). Machine learning applications in healthcare. Journal of Medical AI, 15(3), 234-251. https://doi.org/10.1234/jmai.2023.015"),
        ("mla_citation", "Johnson, Maria. \"Deep Learning for Natural Language Processing.\" Modern Linguistics Review, vol. 42, no. 2, 2023, pp. 156-178."),
        ("chicago_citation", "Brown, Robert L. \"Statistical Methods in Data Science.\" In _Handbook of Modern Analytics_, edited by Sarah Chen and David Wilson, 289-312. New York: Academic Press, 2023."),
        ("ieee_citation", "[1] A. K. Singh et al., \"Efficient Embedding Generation for Large-Scale Document Retrieval,\" _IEEE Transactions on Knowledge and Data Engineering_, vol. 35, no. 8, pp. 1234-1247, Aug. 2023."),
        ("reference_list", r#"## References

1. Anderson, C. (2022). _The Architecture of Knowledge Systems_. Cambridge University Press.
2. Kumar, S., & Lee, J. (2023). Neural information retrieval: A comprehensive survey. _ACM Computing Surveys_, 55(4), 1-38.
3. Wilson, E., & Zhang, L. (2023). Cross-lingual embedding models for multilingual document understanding. _Proceedings of the International Conference on Computational Linguistics_, 1234-1245.
4. Thompson, R. (2022). Vector databases and semantic search: Performance analysis and optimization techniques. _VLDB Journal_, 31(5), 789-812."#),
    ];

    for (description, content) in citation_cases {
        let embedding = harness.generate_embedding(content).await?;

        assert_eq!(
            embedding.len(),
            768,
            "Citation '{}' should produce 768-dimensional embedding",
            description
        );

        // Verify embedding quality
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "Citation '{}' embedding value at index {} should be finite",
                description, i
            );
        }

        let variance = calculate_variance(&embedding);
        assert!(
            variance > 0.0,
            "Citation '{}' embedding should have positive variance",
            description
        );

        println!("Citation '{}' embedding variance: {:.4}", description, variance);
    }

    Ok(())
}

/// Test scholarly language and terminology
///
/// Verifies:
/// - Academic vocabulary is handled
/// - Technical scholarly terms are captured
/// - Complex sentence structures work
/// - Field-specific terminology is understood
#[tokio::test]
async fn test_scholarly_language_terminology() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let scholarly_content_cases = vec![
        ("abstract", r#"## Abstract

This study presents a novel methodology for vector embedding generation in large-scale knowledge management systems. We propose a hybrid approach that combines transformer-based language models with domain-specific fine-tuning to improve semantic search accuracy. Our experimental results demonstrate a 23% improvement in retrieval relevance scores compared to baseline methods across three different domains: academic research, technical documentation, and business correspondence. The methodology addresses key challenges in processing multi-modal content, handling domain-specific terminology, and maintaining semantic consistency across document updates."#),
        ("methodology", r#"## Methodology

### Research Design
We employed a mixed-methods approach combining quantitative analysis with qualitative evaluation. The study was conducted over a 12-month period with a sample size of 250 participants from diverse organizational backgrounds.

### Data Collection
Primary data was collected through structured interviews, document analysis, and system usage metrics. Secondary data included literature review of 75 peer-reviewed articles and analysis of existing embedding generation frameworks.

### Statistical Analysis
We utilized both parametric and non-parametric statistical methods. The significance threshold was set at α = 0.05. Effect sizes were calculated using Cohen's d for pairwise comparisons and η² for ANOVA results."#),
        ("technical_terminology", r#"## Theoretical Framework

The research is grounded in information retrieval theory and natural language processing principles. Key concepts include:

- **Semantic similarity**: Cosine similarity measurement in high-dimensional vector spaces
- **Contextual embeddings**: Transformer-based representations capturing contextual meaning
- **Domain adaptation**: Fine-tuning strategies for specialized vocabularies
- **Multi-modal processing**: Integration of text, code, and structured data representations

The framework extends traditional bag-of-words models by incorporating attention mechanisms and transfer learning approaches."#),
    ];

    for (description, content) in scholarly_content_cases {
        let embedding = harness.generate_embedding(content).await?;

        assert_eq!(
            embedding.len(),
            768,
            "Scholarly content '{}' should produce 768-dimensional embedding",
            description
        );

        // Verify embedding captures academic complexity
        let variance = calculate_variance(&embedding);
        assert!(
            variance > 0.01, // Academic content should have good variance
            "Scholarly content '{}' should have good variance",
            description
        );

        println!("Scholarly content '{}' embedding variance: {:.4}", description, variance);

        // Test academic terminology extraction
        let terms = extract_scholarly_terms(content);
        println!("Found {} scholarly terms in '{}'", terms.len(), description);

        for term in terms.iter().take(5) {
            let term_embedding = harness.generate_embedding(term).await?;
            let similarity = cosine_similarity(&embedding, &term_embedding);
            println!("  Term '{}' similarity: {:.4}", term, similarity);
        }
    }

    Ok(())
}

// ============================================================================
// Business Content Tests
// ============================================================================

/// Test business project management content
///
/// Verifies:
/// - Project plans and timelines are processed
/// - Business terminology is captured
/// - Task and milestone tracking works
/// - Budget and resource information is handled
#[tokio::test]
async fn test_business_project_management_content() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Read project management document from test kiln
    let pm_doc_path = "tests/test-kiln/Project Management.md";
    let pm_content = std::fs::read_to_string(pm_doc_path)?;

    // Generate embedding for project management content
    let pm_embedding = harness.generate_embedding(&pm_content).await?;

    assert_eq!(
        pm_embedding.len(),
        768,
        "Project management document should produce 768-dimensional embedding"
    );

    // Test business-specific sections
    let business_sections = extract_business_sections(&pm_content);

    for (section_type, content) in business_sections {
        let section_embedding = harness.generate_embedding(&content).await?;

        // Compare section with full PM document
        let similarity = cosine_similarity(&pm_embedding, &section_embedding);
        println!("Business section '{}' similarity to full doc: {:.4}", section_type, similarity);

        // Business sections should be related to the full document
        assert!(
            similarity > 0.3,
            "Business section '{}' should be related to full document",
            section_type
        );
    }

    // Test business metrics and data
    let business_metrics = extract_business_metrics(&pm_content);
    println!("Found {} business metrics", business_metrics.len());

    for metric in business_metrics {
        let metric_embedding = harness.generate_embedding(&metric).await?;
        assert_eq!(metric_embedding.len(), 768);

        let similarity = cosine_similarity(&pm_embedding, &metric_embedding);
        println!("Business metric similarity: {:.4}", similarity);
    }

    Ok(())
}

/// Test meeting notes and action items
///
/// Verifies:
/// - Meeting minutes are processed correctly
/// - Action items are captured
/// - Decision records are handled
/// - Attendee and scheduling information works
#[tokio::test]
async fn test_meeting_notes_and_action_items() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Read meeting notes from test kiln
    let meeting_doc_path = "tests/test-kiln/Meeting Notes.md";
    let meeting_content = std::fs::read_to_string(meeting_doc_path)?;

    // Generate embedding for meeting content
    let meeting_embedding = harness.generate_embedding(&meeting_content).await?;

    assert_eq!(
        meeting_embedding.len(),
        768,
        "Meeting notes should produce 768-dimensional embedding"
    );

    // Extract and test meeting components
    let meeting_components = extract_meeting_components(&meeting_content);

    for (component_type, content) in meeting_components {
        let component_embedding = harness.generate_embedding(&content).await?;

        // Compare component with full meeting notes
        let similarity = cosine_similarity(&meeting_embedding, &component_embedding);
        println!("Meeting component '{}' similarity to full doc: {:.4}", component_type, similarity);

        // Meeting components should be related to the full document
        assert!(
            similarity > 0.3,
            "Meeting component '{}' should be related to full document",
            component_type
        );
    }

    // Test specific action items
    let action_items = extract_action_items(&meeting_content);
    println!("Found {} action items", action_items.len());

    for (i, action_item) in action_items.iter().enumerate() {
        let action_embedding = harness.generate_embedding(action_item).await?;
        assert_eq!(action_embedding.len(), 768);

        let similarity = cosine_similarity(&meeting_embedding, &action_embedding);
        println!("Action item {} similarity: {:.4}", i + 1, similarity);
    }

    Ok(())
}

/// Test business financial and timeline data
///
/// Verifies:
/// - Financial information is processed
/// - Timeline and milestone data works
/// - Resource allocation information is captured
/// - Business metrics are handled correctly
#[tokio::test]
async fn test_business_financial_timeline_data() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let business_data_cases = vec![
        ("budget_info", r#"## Budget Allocation

### Q1 2024 Budget
- Development: $150,000
- Marketing: $75,000
- Operations: $50,000
- Research: $100,000
- Contingency: $25,000
**Total: $400,000**

### Monthly Burn Rate
- January: $32,500
- February: $34,200
- March: $31,800
- **Average: $32,833**

### Year-to-Date Expenditure
- Spent: $98,500
- Remaining: $301,500
- **Burn Rate: 24.6%**"#),
        ("timeline_milestones", r#"## Project Timeline

### Phase 1: Foundation (Weeks 1-4)
- Week 1: Project kickoff and team assembly
- Week 2: Requirements gathering and analysis
- Week 3: System architecture design
- Week 4: Development environment setup

### Phase 2: Development (Weeks 5-12)
- Weeks 5-6: Core backend development
- Weeks 7-8: Database implementation
- Weeks 9-10: Frontend development
- Weeks 11-12: API integration

### Phase 3: Testing (Weeks 13-16)
- Weeks 13-14: Unit and integration testing
- Weeks 15-16: User acceptance testing

### Key Milestones
- **M1**: Architecture complete (Week 4)
- **M2**: Backend functional (Week 8)
- **M3**: Frontend complete (Week 12)
- **M4**: Testing complete (Week 16)
- **Go-Live**: Week 17"#),
        ("resource_allocation", r#"## Resource Management

### Team Composition
- **Engineering**: 5 developers (3 senior, 2 junior)
- **Product**: 1 product manager, 1 UX designer
- **QA**: 2 test engineers
- **DevOps**: 1 infrastructure engineer
- **Total Team Size**: 10 people

### Time Allocation
- Development: 60% (240 hours/week)
- Meetings: 15% (60 hours/week)
- Planning: 10% (40 hours/week)
- Testing: 10% (40 hours/week)
- Documentation: 5% (20 hours/week)

### Resource Utilization
- Current utilization: 85%
- Target utilization: 90%
- Available capacity: 40 hours/week
- Critical path: Backend API development"#),
    ];

    for (description, content) in business_data_cases {
        let embedding = harness.generate_embedding(content).await?;

        assert_eq!(
            embedding.len(),
            768,
            "Business data '{}' should produce 768-dimensional embedding",
            description
        );

        // Verify embedding quality
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "Business data '{}' embedding value at index {} should be finite",
                description, i
            );
        }

        let variance = calculate_variance(&embedding);
        assert!(
            variance > 0.0,
            "Business data '{}' embedding should have positive variance",
            description
        );

        println!("Business data '{}' embedding variance: {:.4}", description, variance);

        // Extract numbers and financial data
        let numbers = extract_numbers_from_text(content);
        println!("Found {} numbers in '{}': {:?}", numbers.len(), description, numbers);
    }

    Ok(())
}

// ============================================================================
// Multilingual Content Tests
// ============================================================================

/// Test Unicode and multilingual content
///
/// Verifies:
/// - Various language scripts are processed
        /// - Mixed language documents work
/// - Special characters are handled
/// - Right-to-left text is supported
#[tokio::test]
async fn test_unicode_multilingual_content() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let multilingual_cases = vec![
        ("european_languages", r#"# Multilingual European Content

## French
Crucible est un système de gestion des connaissances qui combine l'organisation hiérarchique, la collaboration en temps réel et l'intégration d'agents IA. Il favorise la **pensée connectée** - la connexion et l'évolution transparentes des idées à travers le temps et le contexte.

## German
Das Crucible-Wissensmanagement-System bietet hierarchische Organisation, Echtzeit-Zusammenarbeit und KI-Agenten-Integration. Es fördert **vernetztes Denken** - die nahtlose Verbindung und Weiterentwicklung von Ideen über Zeit und Kontext hinweg.

## Spanish
Crucible es un sistema de gestión del conocimiento que combina organización jerárquica, colaboración en tiempo real e integración de agentes de IA. Promueve el **pensamiento conectado** - la conexión y evolución fluida de ideas a través del tiempo y el contexto.

## Italian
Crucible è un sistema di gestione della conoscenza che combina organizzazione gerarchica, collaborazione in tempo reale e integrazione di agenti IA. Promuove il **pensiero connesso** - la connessione e l'evoluzione trasparente delle idee nel tempo e nel contesto."#),
        ("asian_languages", r#"# Asian Language Content

## Japanese
クリュービブルは階層型編成、リアルタイム協業、AIエージェント統合を組み合わせたナレッジマネジメントシステムです。**接続思考**を促進します - 時間とコンテキストを超えたアイデアのシームレスな接続と進化。

## Chinese (Simplified)
Crucible是一个结合分层组织、实时协作和AI代理集成的知识管理系统。它促进**连接思维** - 思想在时间和情境中的无缝连接和演进。

## Korean
Crucible는 계층적 구성, 실시간 협업, AI 에이전트 통합을 결합한 지식 관리 시스템입니다. **연결적 사고**를 촉진합니다 - 시간과 맥락을 넘어 아이디어의 원활한 연결과 진화.

## Arabic
نظام إدارة المعرفة Crucible يجمع بين التنظيم الهرمي والتعاون في الوقت الفعلي وتكامل وكلاء الذكاء الاصطناعي. إنه يعزز **التفكير المتصل** - الربط والتطور السلس للأفكار عبر الوقت والسياق."#),
        ("mixed_unicode", r#"# Mixed Unicode Content

## Mathematical Expressions
Einstein's famous equation: E = mc²
Pythagorean theorem: a² + b² = c²
Euler's identity: e^(iπ) + 1 = 0
Standard deviation: σ = √(Σ(xᵢ - μ)² / n)

## Special Characters
- Accents: café, naïve, Noël, résumé
- German Umlauts: Müller, München, Österreich
- Nordic letters: Åland, Øresund, Ærø
- Cyrillic: Москва, Санкт-Петербург, Россия
- Greek: Αθήνα, Θεσσαλονίκη, Ελλάδα

## Symbols and Emojis
Technical symbols: ±, ×, ÷, ≈, ≠, ≤, ≥
Mathematical symbols: ∑, ∏, ∫, ∂, ∇, ∆
Currency symbols: $, €, £, ¥, ₩, ₽
Emojis: 🚀 🎨 🔬 📚 💡 🔗 🌟"#),
        ("right_to_left", r"""# Right-to-Left Content

## Hebrew
קרוסיבל היא מערכת לניהול ידע המשלבת ארגון היררכי, שיתוף פעולה בזמן אמת ושילוב סוכני AI. היא מקדמת **חשיבה מקושרת** - חיבור והתפתחות חלקים של רעיונות לאורך זמן והקשר.

## Farsi
سیستم مدیریت دانش کریسیبل ترکیبی از سازماندهی سلسله مراتبی، همکاری بی‌درنگ و یکپارچه‌سازی عامل‌های هوش مصنوعی است. این سیستم **تفکر متصل** را ترویج می‌دهد - اتصال و تکامل یکپارچه ایده‌ها در طول زمان و بستر.

## Mixed RTL/LTR
English text followed by العربية ثم العبرية ואז עוד אנגלית.
The word **"שלום"** means peace in Hebrew.
In Farsi, **"سلام"** is the greeting.
مرحبا (Marhaba) is Arabic for hello."""#),
    ];

    for (description, content) in multilingual_cases {
        let embedding = harness.generate_embedding(content).await?;

        assert_eq!(
            embedding.len(),
            768,
            "Multilingual content '{}' should produce 768-dimensional embedding",
            description
        );

        // Verify embedding quality
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "Multilingual content '{}' embedding value at index {} should be finite",
                description, i
            );
        }

        let variance = calculate_variance(&embedding);
        assert!(
            variance > 0.0,
            "Multilingual content '{}' embedding should have positive variance",
            description
        );

        println!("Multilingual content '{}' embedding variance: {:.4}", description, variance);

        // Test language detection and character analysis
        let languages = detect_languages(content);
        println!("Detected languages in '{}': {:?}", description, languages);

        let char_types = analyze_character_types(content);
        println!("Character types in '{}': {:?}", description, char_types);
    }

    Ok(())
}

/// Test special characters and symbols
///
/// Verifies:
/// - Mathematical symbols are processed
/// - Currency symbols work
/// - Technical symbols are handled
/// - Emoji content is processed correctly
#[tokio::test]
async fn test_special_characters_symbols() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let special_char_cases = vec![
        ("mathematical", r#"## Mathematical Expressions

### Basic Operations
- Addition: 2 + 3 = 5
- Multiplication: 4 × 5 = 20
- Division: 15 ÷ 3 = 5
- Exponents: 2³ = 8

### Advanced Mathematics
- Square root: √16 = 4
- Infinity: ∞
- Pi: π ≈ 3.14159
- Euler's number: e ≈ 2.71828

### Calculus
- Integral: ∫₀¹ x² dx = 1/3
- Summation: Σᵢ₌₁ⁿ i = n(n+1)/2
- Partial derivative: ∂f/∂x
- Gradient: ∇f

### Set Theory
- Element of: x ∈ S
- Subset: A ⊂ B
- Union: A ∪ B
- Intersection: A ∩ B
- Empty set: ∅"#),
        ("currency", r#"## Currency Information

### Major Currencies
- US Dollar: $1,000.00
- Euro: €850.50
- British Pound: £750.25
- Japanese Yen: ¥150,000
- Chinese Yuan: ¥7,000.00
- Korean Won: ₩1,200,000
- Russian Ruble: ₽75,000

### Exchange Rates
- 1 USD = €0.85
- 1 USD = £0.73
- 1 USD = ¥110.50
- 1 USD = ₩1,180

### Financial Symbols
- Stock ticker: AAPL, GOOGL, MSFT
- Cryptocurrency: ₿1.5 = $50,000
- Percentages: 15.5%, 2.75%, 0.25%
- Financial ratios: P/E = 25.4, ROE = 18.2%"#),
        ("technical_symbols", r#"## Technical Symbols and Notations

### Programming
- Assignment: x ← 5
- Equality: x == y
- Inequality: x ≠ y
- Comparison: x ≤ y, x ≥ y

### Logic
- AND: p ∧ q
- OR: p ∨ q
- NOT: ¬p
- Implies: p → q
- If and only if: p ↔ q

### Units and Measurements
- Temperature: 25°C, 77°F
- Angles: 90°, π/2 rad
- Length: 5m, 10ft, 3in
- Weight: 2.5kg, 5.5lbs

### File Paths and URLs
- Windows: C:\Users\Name\Documents
- Unix: /home/user/documents
- URL: https://example.com/path/to/resource
- Email: user@domain.com"#),
        ("emoji_content", r#"## Emoji-Enhanced Content

### Project Status 🚀
- ✅ Backend API complete
- ✅ Database integration done
- 🔄 Frontend in progress
- ⏳ Testing phase starts next week
- 🎯 Launch date: December 1st

### Team Collaboration 👥
- 👨‍💻 Developers: 5 people
- 🎨 Designers: 2 people
- 📊 Project managers: 1 person
- 🔧 DevOps engineer: 1 person

### Key Metrics 📈
- 📊 Performance: +45%
- ⚡ Speed improvement: 2.3x faster
- 💰 Cost reduction: -30%
- 🎯 Customer satisfaction: 92%

### Priority Tasks 🔥
1. 🐛 Fix critical bugs
2. 🚀 Deploy to production
3. 📚 Update documentation
4. 🧪 Write comprehensive tests
5. 📈 Monitor performance metrics

### Celebrations 🎉
- 🏆 Won innovation award
- ⭐ 5-star customer reviews
- 💎 Featured in tech blog
- 🌟 Exceeded Q3 targets"#),
    ];

    for (description, content) in special_char_cases {
        let embedding = harness.generate_embedding(content).await?;

        assert_eq!(
            embedding.len(),
            768,
            "Special character content '{}' should produce 768-dimensional embedding",
            description
        );

        // Verify embedding quality
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "Special character content '{}' embedding value at index {} should be finite",
                description, i
            );
        }

        let variance = calculate_variance(&embedding);
        assert!(
            variance > 0.0,
            "Special character content '{}' embedding should have positive variance",
            description
        );

        println!("Special character content '{}' embedding variance: {:.4}", description, variance);

        // Analyze character types
        let char_analysis = analyze_special_characters(content);
        println!("Character analysis for '{}': {:?}", description, char_analysis);
    }

    Ok(())
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Extract code snippets from markdown content
fn extract_code_snippets(content: &str) -> Vec<String> {
    let mut snippets = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut in_code_block = false;
    let mut current_snippet = Vec::new();

    for line in lines {
        if line.trim().starts_with("```") {
            if in_code_block {
                // End of code block
                if !current_snippet.is_empty() {
                    snippets.push(current_snippet.join("\n"));
                    current_snippet.clear();
                }
                in_code_block = false;
            } else {
                // Start of code block
                in_code_block = true;
            }
        } else if in_code_block {
            current_snippet.push(line);
        }
    }

    snippets
}

/// Detect programming languages in content
fn detect_programming_languages(content: &str) -> Vec<String> {
    let mut languages = Vec::new();
    let code_snippets = extract_code_snippets(content);

    for snippet in code_snippets {
        // Simple language detection based on keywords
        if snippet.contains("fn ") || snippet.contains("let ") || snippet.contains("impl ") {
            if !languages.contains(&"Rust".to_string()) {
                languages.push("Rust".to_string());
            }
        }
        if snippet.contains("function ") || snippet.contains("const ") || snippet.contains("let ") {
            if !languages.contains(&"JavaScript".to_string()) {
                languages.push("JavaScript".to_string());
            }
        }
        if snippet.contains("def ") || snippet.contains("import ") || snippet.contains("class ") {
            if !languages.contains(&"Python".to_string()) {
                languages.push("Python".to_string());
            }
        }
        if snippet.contains("SELECT ") || snippet.contains("FROM ") || snippet.contains("WHERE ") {
            if !languages.contains(&"SQL".to_string()) {
                languages.push("SQL".to_string());
            }
        }
    }

    languages
}

/// Extract API components from documentation
fn extract_api_components(content: &str) -> Vec<(String, String)> {
    let mut components = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut current_section = String::new();
    let mut current_content = Vec::new();

    for line in lines {
        if line.trim().starts_with('#') {
            // Save previous section if exists
            if !current_section.is_empty() && !current_content.is_empty() {
                components.push((current_section.clone(), current_content.join("\n")));
                current_content.clear();
            }
            current_section = line.trim().to_string();
        } else if !line.trim().is_empty() {
            current_content.push(line);
        }
    }

    // Save last section
    if !current_section.is_empty() && !current_content.is_empty() {
        components.push((current_section, current_content.join("\n")));
    }

    components
}

/// Extract API endpoints from documentation
fn extract_api_endpoints(content: &str) -> Vec<String> {
    let mut endpoints = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    for line in lines {
        // Look for HTTP methods and paths
        if line.trim().starts_with("GET ") ||
           line.trim().starts_with("POST ") ||
           line.trim().starts_with("PUT ") ||
           line.trim().starts_with("DELETE ") {
            endpoints.push(line.trim().to_string());
        }
    }

    endpoints
}

/// Extract academic sections from research content
fn extract_academic_sections(content: &str) -> Vec<(String, String)> {
    let mut sections = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut current_section = String::new();
    let mut current_content = Vec::new();

    for line in lines {
        if line.trim().starts_with('#') {
            // Save previous section if exists
            if !current_section.is_empty() && !current_content.is_empty() {
                sections.push((current_section.clone(), current_content.join("\n")));
                current_content.clear();
            }
            current_section = line.trim().to_string();
        } else if !line.trim().is_empty() {
            current_content.push(line);
        }
    }

    // Save last section
    if !current_section.is_empty() && !current_content.is_empty() {
        sections.push((current_section, current_content.join("\n")));
    }

    sections
}

/// Extract academic terminology from content
fn extract_academic_terminology(content: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let academic_keywords = vec![
        "methodology", "research", "analysis", "systematic", "literature review",
        "qualitative", "quantitative", "hypothesis", "variables", "sample size",
        "statistical", "significance", "correlation", "regression", "validity",
        "reliability", "peer-reviewed", "citation", "bibliography", "framework",
        "theoretical", "empirical", "paradigm", "epistemology", "ontology",
    ];

    for keyword in academic_keywords {
        if content.to_lowercase().contains(&keyword.to_lowercase()) {
            terms.push(keyword.to_string());
        }
    }

    terms
}

/// Extract scholarly terms from content
fn extract_scholarly_terms(content: &str) -> Vec<String> {
    let mut terms = Vec::new();

    // Use regex to find potential scholarly terms (capitalized words, technical terms)
    let re = regex::Regex::new(r"\b[A-Z][a-zA-Z]{4,}\b").unwrap();
    for cap in re.captures_iter(content) {
        terms.push(cap[0].to_string());
    }

    // Remove duplicates and limit to reasonable number
    terms.sort();
    terms.dedup();
    terms.truncate(10);

    terms
}

/// Extract business sections from content
fn extract_business_sections(content: &str) -> Vec<(String, String)> {
    let mut sections = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut current_section = String::new();
    let mut current_content = Vec::new();

    for line in lines {
        if line.trim().starts_with('#') {
            // Save previous section if exists
            if !current_section.is_empty() && !current_content.is_empty() {
                sections.push((current_section.clone(), current_content.join("\n")));
                current_content.clear();
            }
            current_section = line.trim().to_string();
        } else if !line.trim().is_empty() {
            current_content.push(line);
        }
    }

    // Save last section
    if !current_section.is_empty() && !current_content.is_empty() {
        sections.push((current_section, current_content.join("\n")));
    }

    sections
}

/// Extract business metrics from content
fn extract_business_metrics(content: &str) -> Vec<String> {
    let mut metrics = Vec::new();

    // Look for monetary amounts
    let re_money = regex::Regex::new(r"\$\d{1,3}(?:,\d{3})*(?:\.\d{2})?").unwrap();
    for cap in re_money.captures_iter(content) {
        metrics.push(cap[0].to_string());
    }

    // Look for percentages
    let re_percent = regex::Regex::new(r"\d+(?:\.\d+)?%").unwrap();
    for cap in re_percent.captures_iter(content) {
        metrics.push(cap[0].to_string());
    }

    // Look for dates and timeframes
    let re_date = regex::Regex::new(r"\d{4}-\d{2}-\d{2}|\bQ[1-4]\s+\d{4}").unwrap();
    for cap in re_date.captures_iter(content) {
        metrics.push(cap[0].to_string());
    }

    metrics
}

/// Extract meeting components from content
fn extract_meeting_components(content: &str) -> Vec<(String, String)> {
    let mut components = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut current_component = String::new();
    let mut current_content = Vec::new();

    for line in lines {
        if line.trim().starts_with('#') ||
           line.trim().to_lowercase().contains("attendees") ||
           line.trim().to_lowercase().contains("agenda") ||
           line.trim().to_lowercase().contains("action items") ||
           line.trim().to_lowercase().contains("decisions") {
            // Save previous component if exists
            if !current_component.is_empty() && !current_content.is_empty() {
                components.push((current_component.clone(), current_content.join("\n")));
                current_content.clear();
            }
            current_component = line.trim().to_string();
        } else if !line.trim().is_empty() {
            current_content.push(line);
        }
    }

    // Save last component
    if !current_component.is_empty() && !current_content.is_empty() {
        components.push((current_component, current_content.join("\n")));
    }

    components
}

/// Extract action items from meeting content
fn extract_action_items(content: &str) -> Vec<String> {
    let mut action_items = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    for line in lines {
        if line.trim().starts_with("- [ ]") ||
           line.trim().starts_with("- [x]") ||
           line.to_lowercase().contains("action item") ||
           line.to_lowercase().contains("todo") ||
           line.to_lowercase().contains("follow up") {
            action_items.push(line.trim().to_string());
        }
    }

    action_items
}

/// Extract numbers from text
fn extract_numbers_from_text(text: &str) -> Vec<String> {
    let mut numbers = Vec::new();
    let re = regex::Regex::new(r"\b\d{1,3}(?:,\d{3})*(?:\.\d+)?\b").unwrap();

    for cap in re.captures_iter(text) {
        numbers.push(cap[0].to_string());
    }

    numbers
}

/// Detect languages in content
fn detect_languages(content: &str) -> Vec<String> {
    let mut languages = Vec::new();

    // Simple language detection based on character ranges
    if content.contains('א') || content.contains('ב') || content.contains('ג') {
        languages.push("Hebrew".to_string());
    }
    if content.contains('ا') || content.contains('ب') || content.contains('ت') {
        languages.push("Arabic".to_string());
    }
    if content.contains('あ') || content.contains('い') || content.contains('う') {
        languages.push("Japanese".to_string());
    }
    if content.contains('한') || content.contains('글') || content.contains('조') {
        languages.push("Korean".to_string());
    }
    if content.contains('字') || content.contains('文') || content.contains('语') {
        languages.push("Chinese".to_string());
    }
    if content.contains('ç') || content.contains('é') || content.contains('ñ') {
        languages.push("European".to_string());
    }
    if !languages.is_empty() {
        languages.push("English".to_string()); // Assume English if mixed
    }

    if languages.is_empty() {
        languages.push("English".to_string());
    }

    languages
}

/// Analyze character types in content
fn analyze_character_types(content: &str) -> std::collections::HashMap<String, usize> {
    let mut char_types = std::collections::HashMap::new();

    for ch in content.chars() {
        let category = if ch.is_ascii() {
            if ch.is_alphabetic() {
                "ASCII letters"
            } else if ch.is_numeric() {
                "ASCII numbers"
            } else if ch.is_whitespace() {
                "Whitespace"
            } else {
                "ASCII symbols"
            }
        } else if ch.is_alphabetic() {
            "Unicode letters"
        } else if ch.is_numeric() {
            "Unicode numbers"
        } else {
            "Unicode symbols"
        };

        *char_types.entry(category.to_string()).or_insert(0) += 1;
    }

    char_types
}

/// Analyze special characters in content
fn analyze_special_characters(content: &str) -> std::collections::HashMap<String, usize> {
    let mut char_analysis = std::collections::HashMap::new();

    for ch in content.chars() {
        let category = if ch.is_ascii() {
            "ASCII".to_string()
        } else if ch.is_alphabetic() {
            "Unicode letters".to_string()
        } else if ch.is_numeric() {
            "Unicode numbers".to_string()
        } else if (0x1F600..=0x1F64F).contains(&(ch as u32)) {
            "Emojis".to_string()
        } else if (0x2200..=0x22FF).contains(&(ch as u32)) {
            "Math symbols".to_string()
        } else if (0x20A0..=0x20CF).contains(&(ch as u32)) {
            "Currency symbols".to_string()
        } else {
            "Other Unicode".to_string()
        };

        *char_analysis.entry(category).or_insert(0) += 1;
    }

    char_analysis
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