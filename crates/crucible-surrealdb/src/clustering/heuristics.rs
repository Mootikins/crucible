use super::{ClusteringError, DocumentInfo, MocCandidate};

/// Detect Maps of Content using heuristic rules
///
/// A MoC (Map of Content) is typically characterized by:
/// - High number of outbound links (> 5)
/// - Tags indicating it's a map/index
/// - Position as a hub in the link graph
/// - Brief content with many links
pub async fn detect_mocs(documents: &[DocumentInfo]) -> Result<Vec<MocCandidate>, ClusteringError> {
    let mut candidates = Vec::new();

    for doc in documents {
        let score = calculate_moc_score(doc);
        let mut reasons = Vec::new();

        // Check outbound links
        if doc.outbound_links.len() > 5 {
            reasons.push(format!("Has {} outbound links", doc.outbound_links.len()));
        }

        // Check for MoC-related tags
        for tag in &doc.tags {
            let tag_lower = tag.to_lowercase();
            if tag_lower.contains("moc")
                || tag_lower.contains("map-of-content")
                || tag_lower.contains("index")
                || tag_lower.contains("hub") {
                reasons.push(format!("Has tag: {}", tag));
            }
        }

        // Check title patterns
        if let Some(title) = &doc.title {
            let title_lower = title.to_lowercase();
            if title_lower.contains("map of content")
                || title_lower.contains("table of contents")
                || title_lower.contains("index")
                || title_lower.contains("overview") {
                reasons.push(format!("Title suggests MoC: {}", title));
            }
        }

        // Check content pattern (brief with many links)
        if doc.content_length < 1000 && doc.outbound_links.len() > 3 {
            reasons.push("Brief content with many links".to_string());
        }

        // Check if it's a hub (many inbound and outbound links)
        if doc.inbound_links.len() > 3 && doc.outbound_links.len() > 3 {
            reasons.push("Acts as a link hub".to_string());
        }

        // Only include if we have reasons to believe it's a MoC
        if !reasons.is_empty() && score > 0.3 {
            candidates.push(MocCandidate {
                file_path: doc.file_path.clone(),
                score,
                reasons,
                outbound_links: doc.outbound_links.len(),
                inbound_links: doc.inbound_links.len(),
            });
        }
    }

    // Sort by score descending
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

    Ok(candidates)
}

/// Calculate a score indicating how likely a document is a MoC
fn calculate_moc_score(doc: &DocumentInfo) -> f64 {
    let mut score: f64 = 0.0;

    // Outbound links (most important factor)
    if doc.outbound_links.len() > 10 {
        score += 0.4;
    } else if doc.outbound_links.len() > 5 {
        score += 0.2;
    } else if doc.outbound_links.len() > 3 {
        score += 0.1;
    }

    // Inbound links (shows it's referenced by others)
    if doc.inbound_links.len() > 5 {
        score += 0.2;
    } else if doc.inbound_links.len() > 2 {
        score += 0.1;
    }

    // Tags
    for tag in &doc.tags {
        let tag_lower = tag.to_lowercase();
        if tag_lower.contains("moc") {
            score += 0.3;
        }
        if tag_lower.contains("index") || tag_lower.contains("hub") {
            score += 0.2;
        }
    }

    // Title patterns
    if let Some(title) = &doc.title {
        let title_lower = title.to_lowercase();
        if title_lower.contains("map of content") {
            score += 0.4;
        }
        if title_lower.contains("table of contents") || title_lower.contains("toc") {
            score += 0.3;
        }
        if title_lower.contains("overview") || title_lower.contains("summary") {
            score += 0.2;
        }
    }

    // Content characteristics
    if doc.content_length > 0 {
        let link_density = doc.outbound_links.len() as f64 / doc.content_length as f64 * 1000.0;
        if link_density > 10.0 && doc.content_length < 2000 {
            score += 0.2;
        }
    }

    // Normalize to 0.0-1.0 range
    score.min(1.0)
}