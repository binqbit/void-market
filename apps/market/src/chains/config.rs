use std::{env, path::PathBuf};
use url::Url;

#[derive(Clone)]
pub struct LocalnetConfig {
    pub ethereum: Url,
    pub zcash_node: Url,
    pub zcash_wallet: Url,
    pub wallet_cookie: PathBuf,
}

impl LocalnetConfig {
    pub fn from_env() -> Result<Option<Self>, Box<dyn std::error::Error>> {
        match env::var("DEMO_MODE").as_deref() {
            Err(_) | Ok("false") | Ok("") => return Ok(None),
            Ok("true") => {}
            _ => return Err("DEMO_MODE must be true or false".into()),
        }
        Ok(Some(Self {
            ethereum: endpoint("ETHEREUM_RPC_URL", "http://127.0.0.1:8545")?,
            zcash_node: endpoint("ZCASH_NODE_RPC_URL", "http://127.0.0.1:18232")?,
            zcash_wallet: endpoint("ZCASH_WALLET_RPC_URL", "http://127.0.0.1:28232")?,
            wallet_cookie: env::var("ZCASH_WALLET_COOKIE_FILE")?.into(),
        }))
    }
}

fn endpoint(name: &str, default: &str) -> Result<Url, Box<dyn std::error::Error>> {
    let url = Url::parse(&env::var(name).unwrap_or_else(|_| default.to_owned()))?;
    if url.scheme() != "http" || !url.username().is_empty() || url.password().is_some() {
        return Err("Localnet RPC must use HTTP with credentials supplied separately".into());
    }
    Ok(url)
}
