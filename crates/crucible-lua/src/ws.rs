//! WebSocket module for Lua scripts.
//!
//! Provides WebSocket client functionality for persistent bidirectional connections.
//!
//! # Example
//!
//! ```lua
//! local ws = cru.ws.connect("wss://echo.websocket.org")
//! ws:send("hello")
//! local msg = ws:receive()  -- yields until message arrives
//! ws:close()
//! ```

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures_util::{SinkExt, StreamExt};
use mlua::{Lua, Result, Table, UserData, UserDataMethods};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, protocol::CloseFrame},
};

type WsSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tungstenite::Message,
>;
type WsStream = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

/// A WebSocket connection exposed to Lua as userdata.
///
/// Provides `send`, `receive`, and `close` methods for bidirectional messaging.
struct WsConnection {
    sink: Arc<Mutex<Option<WsSink>>>,
    stream: Arc<Mutex<Option<WsStream>>>,
    closed: Arc<Mutex<bool>>,
}

impl UserData for WsConnection {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("send", |_lua, this, payload: String| async move {
            let closed = *this.closed.lock().await;
            if closed {
                return Err(mlua::Error::runtime("WebSocket connection is closed"));
            }

            let mut sink_guard = this.sink.lock().await;
            let sink = sink_guard
                .as_mut()
                .ok_or_else(|| mlua::Error::runtime("WebSocket connection is closed"))?;

            sink.send(tungstenite::Message::Text(payload.into()))
                .await
                .map_err(|e| mlua::Error::runtime(format!("WebSocket error: {e}")))?;

            Ok(())
        });

        methods.add_async_method("send_binary", |_lua, this, payload: String| async move {
            let closed = *this.closed.lock().await;
            if closed {
                return Err(mlua::Error::runtime("WebSocket connection is closed"));
            }

            let bytes = BASE64
                .decode(&payload)
                .map_err(|e| mlua::Error::runtime(format!("Base64 decode error: {e}")))?;

            let mut sink_guard = this.sink.lock().await;
            let sink = sink_guard
                .as_mut()
                .ok_or_else(|| mlua::Error::runtime("WebSocket connection is closed"))?;

            sink.send(tungstenite::Message::Binary(bytes.into()))
                .await
                .map_err(|e| mlua::Error::runtime(format!("WebSocket error: {e}")))?;

            Ok(())
        });

        // receive(timeout_secs?) — optional timeout in seconds
        // Returns message table on success, nil on timeout, error on failure
        methods.add_async_method("receive", |lua, this, timeout_secs: Option<f64>| async move {
            let closed = *this.closed.lock().await;
            if closed {
                return Err(mlua::Error::runtime("WebSocket connection is closed"));
            }

            let mut stream_guard = this.stream.lock().await;
            let stream = stream_guard
                .as_mut()
                .ok_or_else(|| mlua::Error::runtime("WebSocket connection is closed"))?;

            let deadline = timeout_secs.map(|s| tokio::time::Instant::now() + std::time::Duration::from_secs_f64(s));

            loop {
                let next_msg = if let Some(dl) = deadline {
                    match tokio::time::timeout_at(dl, stream.next()).await {
                        Ok(msg) => msg,
                        Err(_) => return Ok(mlua::Value::Nil), // timeout
                    }
                } else {
                    stream.next().await
                };

                match next_msg {
                    Some(Ok(tungstenite::Message::Text(text))) => {
                        let result = lua.create_table()?;
                        result.set("type", "text")?;
                        result.set("data", text.to_string())?;
                        return Ok(mlua::Value::Table(result));
                    }
                    Some(Ok(tungstenite::Message::Binary(data))) => {
                        let result = lua.create_table()?;
                        result.set("type", "binary")?;
                        result.set("data", BASE64.encode(&data))?;
                        return Ok(mlua::Value::Table(result));
                    }
                    Some(Ok(tungstenite::Message::Close(_))) => {
                        *this.closed.lock().await = true;
                        let result = lua.create_table()?;
                        result.set("type", "close")?;
                        return Ok(mlua::Value::Table(result));
                    }
                    Some(Ok(tungstenite::Message::Ping(data))) => {
                        // Respond with Pong to keep the connection alive
                        let mut sink_guard = this.sink.lock().await;
                        if let Some(sink) = sink_guard.as_mut() {
                            let _ = sink.send(tungstenite::Message::Pong(data)).await;
                        }
                        continue;
                    }
                    Some(Ok(tungstenite::Message::Pong(_))) => {
                        continue;
                    }
                    Some(Ok(tungstenite::Message::Frame(_))) => {
                        continue;
                    }
                    Some(Err(e)) => {
                        *this.closed.lock().await = true;
                        return Err(mlua::Error::runtime(format!("WebSocket error: {e}")));
                    }
                    None => {
                        *this.closed.lock().await = true;
                        return Err(mlua::Error::runtime("WebSocket connection is closed"));
                    }
                }
            }
        });

        methods.add_async_method("close", |_lua, this, ()| async move {
            let mut already_closed = this.closed.lock().await;
            if *already_closed {
                return Ok(());
            }
            *already_closed = true;

            let mut sink_guard = this.sink.lock().await;
            if let Some(mut sink) = sink_guard.take() {
                let close_frame = CloseFrame {
                    code: tungstenite::protocol::frame::coding::CloseCode::Normal,
                    reason: "closed by client".into(),
                };
                let _ = sink
                    .send(tungstenite::Message::Close(Some(close_frame)))
                    .await;
                let _ = sink.close().await;
            }

            let mut stream_guard = this.stream.lock().await;
            stream_guard.take();

            Ok(())
        });
    }
}

/// Register the WebSocket module under `cru.ws` and `crucible.ws`.
///
/// Provides `ws.connect(url, opts?)` which returns a `WsConnection` userdata.
pub fn register_ws_module(lua: &Lua) -> Result<()> {
    let ws_table = lua.create_table()?;

    ws_table.set(
        "connect",
        lua.create_async_function(|lua, args: (String, Option<Table>)| async move {
            let (url, opts) = args;

            let timeout_secs: u64 = opts
                .as_ref()
                .and_then(|o| o.get::<u64>("timeout").ok())
                .unwrap_or(30);

            let connect_fut = connect_async(&url);

            let (ws_stream, _response) =
                tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), connect_fut)
                    .await
                    .map_err(|_| mlua::Error::runtime("WebSocket connection timed out"))?
                    .map_err(|e| mlua::Error::runtime(format!("WebSocket error: {e}")))?;

            let (sink, stream) = ws_stream.split();

            let conn = WsConnection {
                sink: Arc::new(Mutex::new(Some(sink))),
                stream: Arc::new(Mutex::new(Some(stream))),
                closed: Arc::new(Mutex::new(false)),
            };

            lua.create_userdata(conn)
        })?,
    )?;

    crate::lua_util::register_in_namespaces(lua, "ws", ws_table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Function;

    #[tokio::test]
    async fn test_ws_module_registration() {
        let lua = Lua::new();
        register_ws_module(&lua).unwrap();

        let cru: Table = lua.globals().get("cru").unwrap();
        let ws: Table = cru.get("ws").unwrap();
        assert!(ws.get::<Function>("connect").is_ok());

        let crucible_ns: Table = lua.globals().get("crucible").unwrap();
        let ws2: Table = crucible_ns.get("ws").unwrap();
        assert!(ws2.get::<Function>("connect").is_ok());
    }

    #[tokio::test]
    async fn test_ws_connect_timeout() {
        let lua = Lua::new();
        register_ws_module(&lua).unwrap();

        let result = lua
            .load(
                r#"
                local ws = cru.ws
                return ws.connect("ws://192.0.2.1:1234", { timeout = 1 })
                "#,
            )
            .eval_async::<mlua::Value>()
            .await;

        assert!(result.is_err(), "Expected connection to fail");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("timed out") || err_msg.contains("WebSocket error"),
            "Expected timeout or connection error, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_ws_send_after_close_errors() {
        let lua = Lua::new();

        let conn = WsConnection {
            sink: Arc::new(Mutex::new(None)),
            stream: Arc::new(Mutex::new(None)),
            closed: Arc::new(Mutex::new(true)),
        };

        let ud = lua.create_userdata(conn).unwrap();
        lua.globals().set("test_conn", ud).unwrap();

        let result = lua.load(r#"test_conn:send("hello")"#).exec_async().await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("closed"),
            "Expected closed error, got: {err_msg}"
        );

        let result = lua.load(r#"test_conn:receive()"#).exec_async().await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("closed"),
            "Expected closed error, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_ws_close_idempotent() {
        let lua = Lua::new();

        let conn = WsConnection {
            sink: Arc::new(Mutex::new(None)),
            stream: Arc::new(Mutex::new(None)),
            closed: Arc::new(Mutex::new(false)),
        };

        let ud = lua.create_userdata(conn).unwrap();
        lua.globals().set("test_conn", ud).unwrap();

        lua.load(r#"test_conn:close()"#).exec_async().await.unwrap();
        lua.load(r#"test_conn:close()"#).exec_async().await.unwrap();
    }
}
