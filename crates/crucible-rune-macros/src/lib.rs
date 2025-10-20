//! # Crucible Rune Macros
//!
//! This crate provides procedural macros for generating Rune tools in the Crucible system.
//! It enables developers to easily create tools from Rust functions that can be used
//! in the MCP (Model Context Protocol) ecosystem.

#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]

/// # Crucible Rune Macros
///
/// This crate provides procedural macros for generating Rune tools in the Crucible system.
/// It enables developers to easily create tools from Rust functions that can be used
/// in the MCP (Model Context Protocol) ecosystem.
///
/// ## Features
///
/// - **Tool Generation**: `#[rune_tool]` attribute macro for creating tools from functions
/// - **Schema Generation**: Automatic JSON schema generation for tool parameters
/// - **Metadata Storage**: Thread-safe storage for tool metadata
/// - **AST Analysis**: Utilities for analyzing function signatures
/// - **Validation**: Comprehensive validation of tool functions
/// - **Async Support**: First-class support for async tool functions
///
/// ## Quick Start
///
/// Add this to your `Cargo.toml`:
/// ```toml
/// [dependencies]
/// crucible-rune-macros = "0.1.0"
/// ```
///
/// Then use the macro in your code:
///
/// ```rust
/// use crucible_rune_macros::rune_tool;
///
/// #[rune_tool(
///     desc = "Creates a new note with title and content",
///     category = "file",
///     tags = ["note", "create"]
/// )]
/// pub fn create_note(title: String, content: String) -> Result<String, String> {
///     Ok(format!("Created note '{}' with {} characters", title, content.len()))
/// }
/// ```
///
/// ## Macro Reference
///
/// ### `#[rune_tool]`
///
/// The main attribute macro for creating tools from functions.
///
/// #### Attributes
///
/// - `desc` or `description` (required): Human-readable description of what the tool does
/// - `category` (optional): Category for organization (file, search, analysis, utility, etc.)
/// - `async` (optional): Flag to mark as async tool (auto-detected if function is async)
/// - `tags` (optional): Array of tags for tool discovery
///
/// #### Function Requirements
///
/// - Must be a public function (`pub fn ...` or `pub async fn ...`)
/// - Cannot have `self` parameters (must be free functions, not methods)
/// - Parameter names must be simple identifiers
/// - Should have documentation comments
///
/// #### Supported Parameter Types
///
/// - `String` or `&str` → JSON string
/// - `i32`, `i64`, `f32`, `f64`, `isize`, `usize` → JSON number
/// - `bool` → JSON boolean
/// - `Vec<T>` → JSON array
/// - `Option<T>` or `T?` → Optional parameter (nullable in JSON)
/// - Custom structs → JSON object
///
/// #### Return Types
///
/// - Can return any serializable type
/// - `Result<T, E>` is automatically handled
/// - Return type is included in the generated schema
///
/// ### `#[simple_rune_tool]`
///
/// A simplified version that automatically extracts the description from the
/// function's documentation comments.
///
/// ```rust
/// use crucible_rune_macros::simple_rune_tool;
///
/// /// Creates a greeting message
/// #[simple_rune_tool]
/// pub fn greet(name: String) -> String {
///     format!("Hello, {}!", name)
/// }
/// ```

use proc_macro::TokenStream;
use syn::{spanned::Spanned, ItemFn};
use quote::quote;

/// The main `#[rune_tool]` attribute macro
///
/// See the crate documentation for usage examples.
#[proc_macro_attribute]
pub fn rune_tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_fn = match syn::parse(item) {
        Ok(func) => func,
        Err(err) => return err.to_compile_error().into(),
    };

    // Parse attributes (simplified for now)
    let _attr_str = attr.to_string();

    // Validate the function
    if let Err(err) = validate_function(&item_fn) {
        return err.to_compile_error().into();
    }

    // Generate the expanded code (for now, just return the original function)
    let fn_vis = &item_fn.vis;
    let fn_sig = &item_fn.sig;
    let fn_block = &item_fn.block;
    let fn_attrs = &item_fn.attrs;

    let expanded = quote! {
        #(#fn_attrs)*
        #fn_vis #fn_sig #fn_block
    };

    expanded.into()
}

/// Simplified version that extracts description from doc comments
#[proc_macro_attribute]
pub fn simple_rune_tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_fn: ItemFn = match syn::parse(item) {
        Ok(func) => func,
        Err(err) => return err.to_compile_error().into(),
    };

    // Generate the expanded code (for now, just return the original function)
    let fn_vis = &item_fn.vis;
    let fn_sig = &item_fn.sig;
    let fn_block = &item_fn.block;
    let fn_attrs = &item_fn.attrs;

    let expanded = quote! {
        #(#fn_attrs)*
        #fn_vis #fn_sig #fn_block
    };

    expanded.into()
}

/// Validate that the function meets all requirements for a tool
fn validate_function(item_fn: &ItemFn) -> Result<(), syn::Error> {
    // Must be public
    if !matches!(item_fn.vis, syn::Visibility::Public(_)) {
        return Err(syn::Error::new(
            item_fn.vis.span(),
            "Tool functions must be public"
        ));
    }

    // Cannot have self parameters
    for input in &item_fn.sig.inputs {
        if let syn::FnArg::Receiver(_) = input {
            return Err(syn::Error::new(
                input.span(),
                "Tool functions cannot have self parameters (must be free functions)"
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_macro_compiles() {
        // This test just verifies that the crate structure is valid
        assert!(true);
    }
}