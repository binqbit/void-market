use crate::error::{Error, Result};
use arbiter_protocol::{NodeProvision, SigningKey, public_key};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{TryRngCore, rngs::OsRng};
use std::{
    fs::{self, DirBuilder, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
};
use uuid::Uuid;
use zeroize::Zeroizing;

fn create(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| Error::Configuration)?;
    file.write_all(bytes).map_err(|_| Error::Configuration)?;
    file.sync_all().map_err(|_| Error::Configuration)
}

pub fn initialize(directory: &Path, deployment_id: Uuid) -> Result<Uuid> {
    DirBuilder::new()
        .mode(0o700)
        .create(directory)
        .map_err(|_| Error::Configuration)?;
    let directory = directory.canonicalize().map_err(|_| Error::Configuration)?;
    let mut key = Zeroizing::new([0u8; 32]);
    let mut token = Zeroizing::new([0u8; 48]);
    OsRng
        .try_fill_bytes(&mut *key)
        .map_err(|_| Error::Configuration)?;
    OsRng
        .try_fill_bytes(&mut *token)
        .map_err(|_| Error::Configuration)?;
    let node_id = Uuid::new_v4();
    let signing = SigningKey::from_bytes(&key);
    let provision = NodeProvision {
        node_id,
        deployment_id,
        name: "Arbiter node".to_owned(),
        public_key: public_key(&signing),
        token_file: directory
            .join("service.token")
            .to_string_lossy()
            .to_string(),
    };
    create(&directory.join("signing.key"), &*key)?;
    create(
        &directory.join("service.token"),
        URL_SAFE_NO_PAD.encode(*token).as_bytes(),
    )?;
    create(
        &directory.join("node.json"),
        &serde_json::to_vec_pretty(&provision).map_err(|_| Error::Configuration)?,
    )?;
    fs::File::open(&directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| Error::Configuration)?;
    Ok(node_id)
}
