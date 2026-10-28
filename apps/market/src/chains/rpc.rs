use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::Client;
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use url::Url;

#[derive(Clone)]
pub struct Rpc {
    client: Client,
    url: Url,
    cookie: Option<PathBuf>,
}

#[derive(Debug)]
pub enum RpcError {
    Transport,
    Rejected(i64),
    InvalidResponse,
}

impl From<RpcError> for crate::http::error::ApiError {
    fn from(_: RpcError) -> Self {
        Self::Unavailable
    }
}

impl Rpc {
    pub fn new(url: Url, cookie: Option<PathBuf>) -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(3))
                .build()?,
            url,
            cookie,
        })
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.call_with_timeout(method, params, Duration::from_secs(10))
            .await
    }

    pub async fn call_with_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, RpcError> {
        let mut request = self
            .client
            .post(self.url.clone())
            .timeout(timeout)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}));
        if let Some(path) = &self.cookie {
            // Reload after a wallet restart; the cookie is intentionally rotated by Zallet.
            let cookie = tokio::fs::read_to_string(path)
                .await
                .map_err(|_| RpcError::Transport)?;
            request = request.header(
                "Authorization",
                format!("Basic {}", STANDARD.encode(cookie.trim())),
            );
        }
        let response = request.send().await.map_err(|_| RpcError::Transport)?;
        let response = response
            .error_for_status()
            .map_err(|_| RpcError::Transport)?;
        let body: Value = response
            .json()
            .await
            .map_err(|_| RpcError::InvalidResponse)?;
        if let Some(error) = body.get("error").filter(|value| !value.is_null()) {
            return Err(RpcError::Rejected(error["code"].as_i64().unwrap_or(0)));
        }
        body.get("result").cloned().ok_or(RpcError::InvalidResponse)
    }
}
