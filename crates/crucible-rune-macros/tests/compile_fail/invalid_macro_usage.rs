//! Tests for invalid macro usage that should fail to compile
//!
//! These tests use the `trybuild` crate to verify that the macro produces
//! helpful compile-time error messages for common mistakes.

use crucible_rune_macros::rune_tool;

// Test 1: Missing description attribute
#[rune_tool()]
pub fn missing_description() {}
//~^ ERROR Tool description is required

// Test 2: Private function
#[rune_tool(desc = "This should fail because function is private")]
fn private_function() {}
//~^ ERROR Tool functions must be public

// Test 3: Method with self parameter
#[rune_tool(desc = "Methods with self are not allowed")]
pub struct TestStruct;
impl TestStruct {
    #[rune_tool(desc = "Method with self")]
    pub fn method_with_self(&self) {}
}
//~^ ERROR Tool functions cannot have self parameters

// Test 4: Reserved parameter name
#[rune_tool(desc = "Uses reserved parameter name")]
pub fn reserved_ctx(ctx: String) {}
//~^ ERROR Parameter name 'ctx' is reserved

// Test 5: Complex parameter pattern
#[rune_tool(desc = "Uses complex parameter pattern")]
pub fn complex_params((a, b): (String, i32)) {}
//~^ ERROR Parameter names must be simple identifiers

// Test 6: Invalid attribute syntax
#[rune_tool(invalid_attribute = "test")]
pub fn invalid_attribute() {}
//~^ ERROR Unknown attribute

// Test 7: Invalid category in tags
#[rune_tool(desc = "Test tool", tags = "not_an_array")]
pub fn invalid_tags_format() {}
//~^ ERROR Expected string literal in array

// Test 8: Non-string default value
#[rune_tool(desc = "Test tool")]
pub fn invalid_default(#[default = 123] param: String) {}
//~^ ERROR Default value must be a string literal

// Test 9: Empty tool name (this would need to be tested differently)
// This can't be directly tested as a compile error since it's derived from function name

// Test 10: Multiple conflicting async specifications
#[rune_tool(desc = "Test tool", async)]
pub async fn double_async() {}
// This should work (function is async and attribute specifies async)

// Test 11: Invalid parameter type
#[rune_tool(desc = "Test with unsupported type")]
pub fn invalid_type(param: std::io::Error) {}
// This might not fail at compile time since we accept unknown types as objects

// Test 12: Empty attribute
#[rune_tool]
pub fn empty_attribute() {}
//~^ ERROR Tool description is required

// Test 13: Invalid category name
#[rune_tool(desc = "Test tool", category = "")]
pub fn empty_category() {}
// This might not fail immediately but should be normalized

// Test 14: Malformed tags
#[rune_tool(desc = "Test tool", tags = ["valid", 123])]
pub fn malformed_tags() {}
//~^ ERROR Expected string literal in array

// Test 15: Invalid description format
#[rune_tool(desc = 123)]
pub fn invalid_description_format() {}
//~^ ERROR Expected string literal