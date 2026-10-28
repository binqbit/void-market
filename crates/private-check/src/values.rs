use crate::{Digest, InputBinding, Result, checks::number::validate_public_number};
use tfhe_math::{
    DataType, EncryptionContext, EvaluationContext, PublicValue, TfheValue, Width,
    codec::{
        EncodedObject, EncodingLimits, encrypt_compact, export_ciphertext,
        import_ciphertext_artifact, import_compact, import_compact_artifact,
    },
};

pub(crate) struct CiphertextArtifact {
    pub value: TfheValue,
    pub wire: EncodedObject,
}

impl CiphertextArtifact {
    pub(crate) fn encrypt(context: &EncryptionContext, value: PublicValue) -> Result<Self> {
        Self::from_value(context.encrypt(value)?)
    }

    pub(crate) fn from_value(value: TfheValue) -> Result<Self> {
        let wire = export_ciphertext(&value, EncodingLimits::default())?;
        Ok(Self { value, wire })
    }

    pub(crate) fn import(
        bytes: &[u8],
        digest: Digest,
        data_type: DataType,
        context: &EvaluationContext,
    ) -> Result<Self> {
        let (value, wire) = import_ciphertext_artifact(
            bytes,
            digest,
            data_type,
            context,
            EncodingLimits::default(),
        )?;
        Ok(Self { value, wire })
    }
}

/// Original compact input and its exact identities. Imported bounds are unverified.
pub struct NumberInput {
    pub(crate) value: TfheValue,
    pub(crate) wire: EncodedObject,
    pub(crate) binding: InputBinding,
}

impl NumberInput {
    /// Local owner entrypoint: enforce the selected clear input range before encryption.
    pub fn encrypt(context: &EncryptionContext, width: Width, value: u64) -> Result<Self> {
        validate_public_number(width, value)?;
        let literal = PublicValue::uint(width, value)?;
        let wire = encrypt_compact(context, literal, EncodingLimits::default())?;
        let value = import_compact(
            wire.as_bytes(),
            wire.digest(),
            literal.data_type(),
            context,
            EncodingLimits::default(),
        )?;
        let binding = InputBinding::new(&wire, context.binding(), width);

        Ok(Self {
            value,
            wire,
            binding,
        })
    }

    /// Structural import does not prove range, ownership or honest encryption.
    pub fn import(
        bytes: &[u8],
        expected_digest: Digest,
        width: Width,
        context: &EncryptionContext,
    ) -> Result<Self> {
        let (value, wire) = import_compact_artifact(
            bytes,
            expected_digest,
            DataType::Uint(width),
            context,
            EncodingLimits::default(),
        )?;
        let binding = InputBinding::new(&wire, context.binding(), width);

        Ok(Self {
            value,
            wire,
            binding,
        })
    }

    pub fn binding(&self) -> InputBinding {
        self.binding
    }

    pub fn encoded(&self) -> &EncodedObject {
        &self.wire
    }
}

impl std::fmt::Debug for NumberInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NumberInput")
            .field("binding", &self.binding)
            .finish_non_exhaustive()
    }
}
