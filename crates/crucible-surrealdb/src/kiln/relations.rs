//! Graph relations (wikilinks, embeds)
//!
//! This module re-exports relation-related functions from kiln_integration.rs.
//! Future work: Move implementations here for better organization.

// Re-export from legacy kiln_integration module
pub use crate::kiln_integration::{
    create_embed_relationships,
    create_wikilink_edges,
    get_embed_metadata,
    get_embed_relations,
    get_embed_with_metadata,
    get_embedded_documents,
    get_embedded_documents_by_type,
    get_embedding_documents,
    get_linked_documents,
    get_documents_by_tag,
    get_wikilink_relations,
    get_wikilinked_documents,
};
