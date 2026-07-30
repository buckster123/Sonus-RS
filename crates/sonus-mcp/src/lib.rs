//! sonus-mcp — MCP-over-stdio, the drop-in replacement for the Python
//! hermes-sonus plugin (same tool names, same argument shapes —
//! docs/hermes-parity.md is the contract).
//!
//! Wire format: newline-delimited JSON-RPC (the Cerebro/ApexOS format,
//! mirrored from the field-proven Imaginarium MCP). Serial loop — long
//! waits belong in check_status_until_done, whose timeout is resumable.

mod args;
pub mod dispatch;
pub mod tools;
mod transport;

use std::sync::Arc;

use anyhow::Result;
use tracing::info;

use transport::{Frame, StdioTransport};

/// Run the MCP stdio server until stdin closes.
pub async fn run() -> Result<()> {
    let state = Arc::new(dispatch::State::from_env().map_err(|e| anyhow::anyhow!(e))?);
    info!(
        base = %state.cfg.api_base,
        key = if state.client.is_some() { "present" } else { "ABSENT" },
        downloads = %state.library.dir().display(),
        "sonus-mcp up ({} tools)",
        tools::all_tool_schemas().len(),
    );

    let mut transport = StdioTransport::new();

    let init_req = match transport.read().await? {
        Frame::Value(v) => v,
        Frame::Eof => {
            info!("stdin closed before initialize");
            return Ok(());
        }
        Frame::ParseError(e) => {
            tracing::error!("malformed initialize: {e}");
            transport.write(&dispatch::parse_error()).await?;
            return Ok(());
        }
        Frame::Oversized { bytes } => {
            tracing::error!("oversized initialize ({bytes} bytes)");
            transport.write(&dispatch::parse_error()).await?;
            return Ok(());
        }
    };

    let init_resp = if init_req["method"].as_str() == Some("initialize") {
        dispatch::handle_initialize(&init_req)
    } else {
        dispatch::method_not_found(&init_req)
    };
    transport.write(&init_resp).await?;

    loop {
        let msg = match transport.read().await {
            Err(e) => {
                tracing::error!("transport IO: {e}");
                break;
            }
            Ok(Frame::Eof) => break,
            Ok(Frame::ParseError(e)) => {
                tracing::warn!("malformed frame: {e}");
                transport.write(&dispatch::parse_error()).await?;
                continue;
            }
            Ok(Frame::Oversized { bytes }) => {
                tracing::warn!("oversized frame ({bytes} bytes)");
                transport.write(&dispatch::parse_error()).await?;
                continue;
            }
            Ok(Frame::Value(v)) => v,
        };

        let is_notification = msg["id"].is_null()
            || msg["method"]
                .as_str()
                .map(|m| m.starts_with("notifications/"))
                .unwrap_or(false);
        if is_notification {
            continue;
        }

        let method = msg["method"].as_str().unwrap_or("").to_string();
        let resp = match method.as_str() {
            "tools/list" => dispatch::tools_list(&msg),
            "tools/call" => dispatch::dispatch_tool(msg, Arc::clone(&state)).await,
            "ping" => serde_json::json!({
                "jsonrpc": "2.0",
                "id": msg["id"],
                "result": {}
            }),
            _ => dispatch::method_not_found(&msg),
        };
        transport.write(&resp).await?;
    }

    info!("sonus-mcp exiting");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn initialize_schema() {
        let req = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
        let resp = dispatch::handle_initialize(&req);
        assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(resp["result"]["serverInfo"]["name"], "sonus-mcp");
    }

    #[tokio::test]
    async fn unknown_tool_is_a_protocol_error() {
        let state =
            Arc::new(dispatch::State::from_env().expect("stateless env resolve never fails"));
        let msg = json!({
            "jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": {"name": "make_coffee", "arguments": {}}
        });
        let resp = dispatch::dispatch_tool(msg, state).await;
        assert_eq!(resp["error"]["code"], -32602);
        assert!(resp["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unknown tool"));
    }

    #[tokio::test]
    async fn not_yet_tools_answer_honestly_as_results() {
        let state = Arc::new(dispatch::State::from_env().unwrap());
        let msg = json!({
            "jsonrpc": "2.0", "id": 8, "method": "tools/call",
            "params": {"name": "separate_stems", "arguments": {}}
        });
        let resp = dispatch::dispatch_tool(msg, state).await;
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let payload: serde_json::Value = serde_json::from_str(text).unwrap();
        assert!(payload["error"]
            .as_str()
            .unwrap()
            .contains("not implemented yet in Sonus-RS"));
    }
}
