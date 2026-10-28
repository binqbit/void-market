use crate::error::{Error, Result};
use arbiter_protocol::ArtifactRef;
use private_check::{CheckContext, Comparison, InputBinding, OwnerContributionBinding, OwnerId};
use serde::{Deserialize, Serialize};
use tfhe_math::{KeyBinding, Width};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Assurance {
    HonestRoleMechanics,
    ProofRequired,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyMaterial {
    pub configuration: [u8; 32],
    pub public_fingerprint: [u8; 32],
    pub evaluation_fingerprint: [u8; 32],
    pub public_key: ArtifactRef,
    pub evaluation_key: ArtifactRef,
}

impl KeyMaterial {
    pub fn binding(&self) -> Result<KeyBinding> {
        Ok(KeyBinding::from_fingerprints(
            self.configuration,
            self.public_fingerprint,
            self.evaluation_fingerprint,
        )?)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckManifest {
    pub schema_version: u8,
    pub assurance: Assurance,
    pub external_context: [u8; 32],
    pub owners: [Uuid; 2],
    pub width: u8,
    pub comparison: Predicate,
    pub keys: [KeyMaterial; 2],
    pub inputs: [ArtifactRef; 2],
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Predicate {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl Predicate {
    pub fn operation(self) -> Comparison {
        match self {
            Self::Equal => Comparison::Equal,
            Self::NotEqual => Comparison::NotEqual,
            Self::Less => Comparison::Less,
            Self::LessOrEqual => Comparison::LessOrEqual,
            Self::Greater => Comparison::Greater,
            Self::GreaterOrEqual => Comparison::GreaterOrEqual,
        }
    }
}

pub fn width(bits: u8) -> Result<Width> {
    match bits {
        8 => Ok(Width::U8),
        16 => Ok(Width::U16),
        32 => Ok(Width::U32),
        64 => Ok(Width::U64),
        _ => Err(Error::Response),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextRecord {
    pub manifest: CheckManifest,
    pub nonce: [u8; 32],
    pub native_inputs: [[u8; 32]; 2],
}

impl ContextRecord {
    pub fn restore(&self) -> Result<CheckContext> {
        let manifest = &self.manifest;
        let source = InputBinding::from_parts(
            manifest.inputs[0].digest,
            self.native_inputs[0],
            manifest.keys[0].binding()?,
            width(manifest.width)?,
        );
        let target = InputBinding::from_parts(
            manifest.inputs[1].digest,
            self.native_inputs[1],
            manifest.keys[1].binding()?,
            width(manifest.width)?,
        );
        Ok(CheckContext::from_parts(
            manifest.external_context,
            self.nonce,
            [
                OwnerId::new(*manifest.owners[0].as_bytes())?,
                OwnerId::new(*manifest.owners[1].as_bytes())?,
            ],
            manifest.comparison.operation(),
            source,
            target,
        )?)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contribution {
    pub owner: Uuid,
    pub commitment: [u8; 32],
    pub ciphertext: ArtifactRef,
}

impl Contribution {
    pub fn binding(&self, context: [u8; 32], base: [u8; 32]) -> Result<OwnerContributionBinding> {
        Ok(OwnerContributionBinding {
            context,
            base,
            owner: OwnerId::new(*self.owner.as_bytes())?,
            commitment: self.commitment,
        })
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opening {
    pub owner: Uuid,
    pub commitment: [u8; 32],
    pub value: u64,
    pub blinding: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Number {
        context: ContextRecord,
        challenge: ArtifactRef,
        reference: [u8; 32],
    },
    Masks {
        context: ContextRecord,
        base: [u8; 32],
    },
    AnswerCommitment {
        context: ContextRecord,
        base: [u8; 32],
        challenge: ArtifactRef,
        reference: [u8; 32],
    },
    AnswerReveal {
        context: [u8; 32],
        challenge: [u8; 32],
        commitment: [u8; 32],
    },
    Openings {
        context: [u8; 32],
        base: [u8; 32],
    },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Response {
    Number {
        context: [u8; 32],
        challenge: [u8; 32],
        ciphertext: ArtifactRef,
    },
    Masks {
        context: [u8; 32],
        base: [u8; 32],
        contributions: [Contribution; 2],
    },
    AnswerCommitment {
        context: [u8; 32],
        challenge: [u8; 32],
        commitment: [u8; 32],
    },
    AnswerReveal {
        context: [u8; 32],
        challenge: [u8; 32],
        value: u64,
    },
    Openings {
        context: [u8; 32],
        base: [u8; 32],
        openings: [Opening; 2],
    },
}
