pub mod config;
pub mod ethereum;
pub mod rpc;
pub mod zcash;

pub struct Localnet {
    pub ethereum: ethereum::Ethereum,
    pub zcash: zcash::Zcash,
}

impl Localnet {
    pub fn new(config: &config::LocalnetConfig) -> Result<Self, reqwest::Error> {
        Ok(Self {
            ethereum: ethereum::Ethereum::new(config.ethereum.clone())?,
            zcash: zcash::Zcash::new(config)?,
        })
    }
}
