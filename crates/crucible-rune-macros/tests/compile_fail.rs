//! Compile tests using trybuild to verify error messages
//!
//! This test suite verifies that the macro produces helpful compile-time
//! error messages for common usage mistakes.

#[cfg(test)]
mod tests {
    #[test]
    fn test_compile_failures() {
        // The trybuild crate will compile the tests in the compile_fail directory
        // and verify that they produce the expected compile errors
        let t = trybuild::TestCases::new();
        t.compile_fail("tests/compile_fail/*.rs");
    }
}