//! JSON-line RPC server over a Unix-domain socket. One request per
//! connection — `{id, method, params}` in, `{id, ok}` or
//! `{id, err: {code, message}}` out, then EOF. Method dispatch is
//! explicit; unknown methods return an `unknown_method` error.

use crate::agent_complete;
use anyhow::{Context as _, Result, anyhow};
use futures::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use gpui::AsyncApp;
use net::async_net::{UnixListener, UnixStream};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct RpcRequest {
    id: serde_json::Value,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum RpcResponse<'a> {
    Ok {
        id: &'a serde_json::Value,
        ok: serde_json::Value,
    },
    Err {
        id: &'a serde_json::Value,
        err: RpcErrBody,
    },
}

#[derive(Debug, Serialize)]
struct RpcErrBody {
    code: &'static str,
    message: String,
}

pub async fn run(listener: UnixListener, cx: AsyncApp) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let handler_cx = cx.clone();
                cx.foreground_executor()
                    .spawn(async move {
                        if let Err(err) = handle(stream, handler_cx).await {
                            log::warn!("codon-fish: rpc handler error: {err:#}");
                        }
                    })
                    .detach();
            }
            Err(err) => {
                log::warn!("codon-fish: accept failed, stopping listener: {err}");
                return;
            }
        }
    }
}

async fn handle(stream: UnixStream, cx: AsyncApp) -> Result<()> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let bytes = reader
        .read_line(&mut line)
        .await
        .context("read rpc request line")?;
    if bytes == 0 {
        return Ok(());
    }
    let req: RpcRequest = serde_json::from_str(line.trim())
        .with_context(|| format!("parse rpc request: {}", line.trim()))?;
    let result = dispatch(&req, cx).await;
    let response = match result {
        Ok(ok) => RpcResponse::Ok { id: &req.id, ok },
        Err(DispatchError { code, message }) => RpcResponse::Err {
            id: &req.id,
            err: RpcErrBody { code, message },
        },
    };
    let mut payload = serde_json::to_string(&response).context("serialize rpc response")?;
    payload.push('\n');
    let mut writer = reader.into_inner();
    writer
        .write_all(payload.as_bytes())
        .await
        .context("write rpc response")?;
    writer.flush().await.context("flush rpc response")?;
    Ok(())
}

struct DispatchError {
    code: &'static str,
    message: String,
}

impl From<anyhow::Error> for DispatchError {
    fn from(err: anyhow::Error) -> Self {
        Self {
            code: "internal",
            message: format!("{err:#}"),
        }
    }
}

async fn dispatch(req: &RpcRequest, cx: AsyncApp) -> Result<serde_json::Value, DispatchError> {
    match req.method.as_str() {
        "agent.complete" => agent_complete::handle(req.params.clone(), cx)
            .await
            .map_err(Into::into),
        "health.ping" => Ok(serde_json::json!({ "pong": true })),
        other => Err(DispatchError {
            code: "unknown_method",
            message: anyhow!("unknown method `{other}`").to_string(),
        }),
    }
}
