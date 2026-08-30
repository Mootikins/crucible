//! HTTP module for Lua scripts.
//!
//! Provides HTTP client functionality for making network requests from Lua scripts.
//!
//! # Example
//!
//! ```lua
//! -- Simple GET request
//! local response = http.get("https://api.example.com/data")
//! if response.ok then
//!     local data = oq.parse(response.body)
//!     print(data.name)
//! end
//!
//! -- POST request with JSON body
//! local response = http.post("https://api.example.com/users", {
//!     headers = { ["Content-Type"] = "application/json" },
//!     body = oq.json({ name = "Alice", age = 30 })
//! })
//!
//! -- Custom request with full control
//! local response = http.request({
//!     url = "https://api.example.com/resource",
//!     method = "PUT",
//!     headers = { Authorization = "Bearer token123" },
//!     body = "data payload",
//!     timeout = 60
//! })
//! ```
//!
//! # Response Format
//!
//! All HTTP functions return a table with:
//! - `status` - HTTP status code (number)
//! - `headers` - Response headers (table)
//! - `body` - Response body (string)
//! - `ok` - Whether status is 2xx (boolean)
//! - `error` - Error message if request failed (string, only on error)

use crate::error::LuaResult;
use crucible_core::http::{HttpExecutor, HttpMethod, HttpRequest};
use mlua::{Lua, Result, Table};
use std::sync::Arc;

/// The options every request function reads (`build_request`). `timeout` is
/// in SECONDS — the closure feeds `Duration::from_secs`.
const REQUEST_OPTIONS: &str =
    "{ headers: table<string, string>?, body: string?, timeout: number? }";

/// The table every request function answers with (`execute_request`).
///
/// `error` appears only on the failure path, which is why it is optional; the
/// other four keys are set on both paths, so a caller reads `ok` rather than
/// testing for `error`.
const RESPONSE: &str = "{ status: number, ok: boolean, headers: table<string, string>, \
                         body: string, error: string? }";

/// Register HTTP module with Lua.
///
/// Provides `http.get`, `http.post`, `http.put`, `http.delete`, `http.patch`
/// and `http.request`.
///
/// Every function declares its Luau type beside its closure, and `Ns` holds
/// the declaration to the Rust types at registration. See
/// [`crate::host_registry`].
pub fn register_http_module(lua: &Lua) -> LuaResult<()> {
    let mut http = crate::host_registry::Ns::new(lua, "cru.http")?;
    let executor = Arc::new(HttpExecutor::new());

    // http.get / post / put / delete / patch (url, opts?)
    //
    // The method is fixed by the name, so none of these five read
    // `opts.method`; `http.request` is the one that does.
    let declaration = format!("(url: string, options: {REQUEST_OPTIONS}?) -> {RESPONSE}");
    for (name, method) in [
        ("get", HttpMethod::Get),
        ("post", HttpMethod::Post),
        ("put", HttpMethod::Put),
        ("delete", HttpMethod::Delete),
        ("patch", HttpMethod::Patch),
    ] {
        let exec = executor.clone();
        http.async_func(
            name,
            &declaration,
            move |lua, args: (String, Option<Table>)| {
                let exec = exec.clone();
                async move {
                    let (url, opts) = args;
                    let req = build_request(method, url, opts)?;
                    execute_request(&lua, exec, req).await
                }
            },
        )?;
    }

    // http.request(opts) - full control
    //
    // `url` is REQUIRED: the closure reads it with `?`, so a table without one
    // raises. `method` defaults to GET, and an unrecognised name also reads as
    // GET (`parse_method`).
    let exec = executor.clone();
    http.async_func(
        "request",
        &format!(
            "(options: {{ url: string, method: string?, \
             headers: table<string, string>?, body: string?, timeout: number? }}) \
             -> {RESPONSE}"
        ),
        move |lua, opts: Table| {
            let exec = exec.clone();
            async move {
                let url: String = opts.get("url")?;
                let method_str: Option<String> = opts.get("method")?;
                let method = parse_method(method_str.as_deref());
                let req = build_request(method, url, Some(opts))?;
                execute_request(&lua, exec, req).await
            }
        },
    )?;

    http.publish()?;
    Ok(())
}

/// Parse HTTP method string to enum.
fn parse_method(method: Option<&str>) -> HttpMethod {
    match method {
        Some("POST") | Some("post") => HttpMethod::Post,
        Some("PUT") | Some("put") => HttpMethod::Put,
        Some("PATCH") | Some("patch") => HttpMethod::Patch,
        Some("DELETE") | Some("delete") => HttpMethod::Delete,
        Some("HEAD") | Some("head") => HttpMethod::Head,
        Some("OPTIONS") | Some("options") => HttpMethod::Options,
        _ => HttpMethod::Get,
    }
}

/// Build an HTTP request from Lua arguments.
fn build_request(method: HttpMethod, url: String, opts: Option<Table>) -> Result<HttpRequest> {
    let mut req = HttpRequest {
        url,
        method,
        headers: std::collections::HashMap::new(),
        body: None,
        timeout: std::time::Duration::from_secs(30),
    };

    if let Some(opts) = opts {
        // Headers
        if let Ok(headers) = opts.get::<Table>("headers") {
            for pair in headers.pairs::<String, String>() {
                let (k, v) = pair?;
                req.headers.insert(k, v);
            }
        }

        // Body
        if let Ok(body) = opts.get::<String>("body") {
            req.body = Some(body);
        }

        // Timeout (in seconds)
        if let Ok(timeout) = opts.get::<u64>("timeout") {
            req.timeout = std::time::Duration::from_secs(timeout);
        }
    }

    Ok(req)
}

/// Execute an HTTP request and return a Lua table response.
async fn execute_request(
    lua: &Lua,
    executor: Arc<HttpExecutor>,
    req: HttpRequest,
) -> Result<Table> {
    match executor.execute(req).await {
        Ok(response) => {
            let table = lua.create_table()?;
            table.set("status", response.status)?;
            table.set("ok", response.is_success())?;

            let headers = lua.create_table()?;
            for (k, v) in &response.headers {
                headers.set(k.clone(), v.clone())?;
            }
            table.set("headers", headers)?;
            table.set("body", response.body)?;

            Ok(table)
        }
        Err(e) => {
            let table = lua.create_table()?;
            table.set("error", e.to_string())?;
            table.set("ok", false)?;
            table.set("status", 0)?;
            table.set("body", "")?;
            table.set("headers", lua.create_table()?)?;
            Ok(table)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Function;

    #[tokio::test]
    async fn test_http_module_registration() {
        let lua = Lua::new();
        register_http_module(&lua).unwrap();

        let cru: Table = lua.globals().get("cru").unwrap();
        let http: Table = cru.get("http").unwrap();
        assert!(http.get::<Function>("get").is_ok());
        assert!(http.get::<Function>("post").is_ok());
        assert!(http.get::<Function>("put").is_ok());
        assert!(http.get::<Function>("delete").is_ok());
        assert!(http.get::<Function>("patch").is_ok());
        assert!(http.get::<Function>("request").is_ok());
    }

    #[test]
    fn test_parse_method() {
        assert_eq!(parse_method(Some("GET")), HttpMethod::Get);
        assert_eq!(parse_method(Some("get")), HttpMethod::Get);
        assert_eq!(parse_method(Some("POST")), HttpMethod::Post);
        assert_eq!(parse_method(Some("post")), HttpMethod::Post);
        assert_eq!(parse_method(Some("PUT")), HttpMethod::Put);
        assert_eq!(parse_method(Some("DELETE")), HttpMethod::Delete);
        assert_eq!(parse_method(Some("PATCH")), HttpMethod::Patch);
        assert_eq!(parse_method(None), HttpMethod::Get);
    }

    #[test]
    fn test_build_request_simple() {
        let req = build_request(HttpMethod::Get, "https://example.com".to_string(), None).unwrap();
        assert_eq!(req.url, "https://example.com");
        assert_eq!(req.method, HttpMethod::Get);
        assert!(req.headers.is_empty());
        assert!(req.body.is_none());
    }

    #[tokio::test]
    async fn test_build_request_with_options() {
        let lua = Lua::new();
        let opts = lua.create_table().unwrap();

        let headers = lua.create_table().unwrap();
        headers.set("Authorization", "Bearer token").unwrap();
        opts.set("headers", headers).unwrap();
        opts.set("body", r#"{"key": "value"}"#).unwrap();
        opts.set("timeout", 60u64).unwrap();

        let req = build_request(
            HttpMethod::Post,
            "https://api.example.com".to_string(),
            Some(opts),
        )
        .unwrap();

        assert_eq!(req.method, HttpMethod::Post);
        assert_eq!(
            req.headers.get("Authorization"),
            Some(&"Bearer token".to_string())
        );
        assert_eq!(req.body, Some(r#"{"key": "value"}"#.to_string()));
        assert_eq!(req.timeout, std::time::Duration::from_secs(60));
    }
}
