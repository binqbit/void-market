use std::{env, fs, os::unix::fs::PermissionsExt, path::PathBuf};
use zeroize::Zeroizing;

pub struct CustodyConfig {
    pub master_key: Zeroizing<[u8; 32]>,
    pub artifacts: PathBuf,
}

impl CustodyConfig {
    pub fn from_env() -> Result<Option<Self>, Box<dyn std::error::Error>> {
        match env::var("DEV_CUSTODY").ok().as_deref() {
            None | Some("") | Some("false") => return Ok(None),
            Some("true") => {}
            _ => return Err("DEV_CUSTODY must be true or false".into()),
        }
        let path = env::var("CUSTODY_MASTER_KEY_FILE")?;
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.len() != 32 || metadata.permissions().mode() & 0o077 != 0
        {
            return Err("Custody master key must be a private 32-byte file".into());
        }
        let bytes = Zeroizing::new(fs::read(path)?);
        let key: [u8; 32] = bytes.as_slice().try_into()?;
        Ok(Some(Self {
            master_key: Zeroizing::new(key),
            artifacts: env::var("CUSTODY_ARTIFACT_ROOT")?.into(),
        }))
    }
}
