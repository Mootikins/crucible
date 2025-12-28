//! IR transformation layer.
//!
//! Transforms modify the GraphIR before rendering. Examples:
//! - Validation
//! - Filter translation (jaq -> IR filters)
//! - Optimization

mod filter;
mod validate;

pub use filter::FilterTransform;
pub use validate::ValidateTransform;

use crate::error::TransformError;
use crate::ir::GraphIR;

/// Trait for IR transformations.
///
/// Transforms are applied in sequence after parsing and before rendering.
pub trait QueryTransform: Send + Sync {
    /// Unique name for this transform
    fn name(&self) -> &'static str;

    /// Transform the IR, returning a modified version
    fn transform(&self, ir: GraphIR) -> Result<GraphIR, TransformError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct IdentityTransform;

    impl QueryTransform for IdentityTransform {
        fn name(&self) -> &'static str {
            "identity"
        }

        fn transform(&self, ir: GraphIR) -> Result<GraphIR, TransformError> {
            Ok(ir)
        }
    }

    #[test]
    fn test_identity_transform() {
        let transform = IdentityTransform;
        let ir = GraphIR::default();
        let result = transform.transform(ir).unwrap();

        assert!(result.pattern.elements.is_empty());
    }
}
