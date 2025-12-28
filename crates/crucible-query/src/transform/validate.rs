//! Validation transform.
//!
//! Validates that the GraphIR is well-formed before rendering.

use crate::error::TransformError;
use crate::ir::GraphIR;
use crate::transform::QueryTransform;

/// Validation transform that checks IR consistency.
pub struct ValidateTransform;

impl QueryTransform for ValidateTransform {
    fn name(&self) -> &'static str {
        "validate"
    }

    fn transform(&self, ir: GraphIR) -> Result<GraphIR, TransformError> {
        // TODO: Add validation rules
        // For now, just pass through
        Ok(ir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_default_ir() {
        let transform = ValidateTransform;
        let ir = GraphIR::default();
        let result = transform.transform(ir);

        assert!(result.is_ok());
    }
}
