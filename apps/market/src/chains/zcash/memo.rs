use crate::http::error::{ApiError, Result};
use minicbor::Encoder;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub struct MemoBinding {
    pub obligation_kind: u8,
    pub intent_class: u8,
    pub network: [u8; 32],
    pub context: [u8; 32],
    pub obligation: [u8; 32],
    pub invoice: [u8; 32],
    pub issue: [u8; 32],
    pub invoice_nonce: [u8; 32],
    pub intent: [u8; 32],
    pub intent_nonce: [u8; 32],
    pub recipient: [u8; 32],
    pub revision: u64,
    pub generation: u64,
}

impl MemoBinding {
    pub fn encode(&self) -> Result<[u8; 512]> {
        if !matches!(self.obligation_kind, 1 | 2)
            || !matches!(self.intent_class, 1..=3)
            || self.revision == 0
        {
            return Err(ApiError::Internal);
        }
        let mut bytes = [0; 512];
        bytes[0] = 0xff;
        bytes[1..8].copy_from_slice(b"VMINV01");
        bytes[8..10].copy_from_slice(&1u16.to_be_bytes());
        bytes[10] = self.obligation_kind;
        bytes[11] = self.intent_class;
        for (offset, value) in [
            (16, &self.network),
            (48, &self.context),
            (80, &self.obligation),
            (112, &self.invoice),
            (144, &self.issue),
            (176, &self.invoice_nonce),
            (208, &self.intent),
            (240, &self.intent_nonce),
            (272, &self.recipient),
        ] {
            bytes[offset..offset + 32].copy_from_slice(value);
        }
        bytes[304..312].copy_from_slice(&self.revision.to_be_bytes());
        bytes[312..314].copy_from_slice(&2u16.to_be_bytes());
        bytes[314..322].copy_from_slice(&self.generation.to_be_bytes());
        Ok(bytes)
    }
}

pub fn reference(tag: &str, field: &str) -> Result<[u8; 32]> {
    let mut item = Encoder::new(Vec::new());
    item.str(field).map_err(|_| ApiError::Internal)?;
    let mut encoded = Encoder::new(Vec::new());
    encoded
        .array(3)
        .and_then(|e| e.str("VM_MEMO_REF_V1"))
        .and_then(|e| e.str(tag))
        .and_then(|e| e.bytes(&item.into_writer()))
        .map_err(|_| ApiError::Internal)?;
    Ok(Sha256::digest(encoded.into_writer()).into())
}

pub fn id_reference(tag: &str, id: Uuid) -> Result<[u8; 32]> {
    reference(tag, &id.to_string())
}
