use crate::error::{Error, Result};
use arbiter_protocol::ArtifactRef;
use rand::{TryRngCore, rngs::OsRng};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct Artifacts {
    root: PathBuf,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    blinding: [u8; 32],
    record: T,
}

impl Artifacts {
    pub fn open(root: &Path) -> Result<Self> {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)
            .map_err(|_| Error::Artifact)?;
        let root = root.canonicalize().map_err(|_| Error::Artifact)?;
        Ok(Self { root })
    }

    pub fn read(&self, reference: &ArtifactRef, limit: u64) -> Result<Zeroizing<Vec<u8>>> {
        let path = self.root.join(format!("{}.bin", reference.id));
        let metadata = fs::symlink_metadata(&path).map_err(|_| Error::Artifact)?;
        if !metadata.is_file() || metadata.len() > limit {
            return Err(Error::Artifact);
        }

        let mut bytes = Zeroizing::new(Vec::new());
        File::open(path)
            .map_err(|_| Error::Artifact)?
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Artifact)?;
        if bytes.len() as u64 > limit
            || <[u8; 32]>::from(Sha256::digest(&*bytes)) != reference.digest
        {
            return Err(Error::Artifact);
        }
        Ok(bytes)
    }

    pub fn json<T: DeserializeOwned>(&self, reference: &ArtifactRef) -> Result<T> {
        serde_json::from_slice(&self.read(reference, 64 * 1024)?).map_err(|_| Error::Artifact)
    }

    pub fn write(&self, bytes: &[u8]) -> Result<ArtifactRef> {
        let id = Uuid::new_v4();
        let path = self.root.join(format!("{id}.bin"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|_| Error::Artifact)?;
        file.write_all(bytes).map_err(|_| Error::Artifact)?;
        file.sync_all().map_err(|_| Error::Artifact)?;
        File::open(&self.root)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| Error::Artifact)?;
        Ok(ArtifactRef {
            id,
            digest: Sha256::digest(bytes).into(),
        })
    }

    pub fn write_json<T: Serialize>(&self, value: &T) -> Result<ArtifactRef> {
        // A private random blinding prevents metadata hashes from enumerating small results.
        let mut blinding = [0u8; 32];
        OsRng
            .try_fill_bytes(&mut blinding)
            .map_err(|_| Error::Artifact)?;
        let bytes = Zeroizing::new(
            serde_json::to_vec(&serde_json::json!({"blinding":blinding,"record":value}))
                .map_err(|_| Error::Artifact)?,
        );
        self.write(&bytes)
    }

    pub fn record<T: DeserializeOwned>(&self, reference: &ArtifactRef) -> Result<T> {
        let envelope: Envelope<T> = self.json(reference)?;
        let _ = envelope.blinding;
        Ok(envelope.record)
    }
}
