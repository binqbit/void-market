use super::{TransferBinding, TransferChallenge, TransferResponse};
use crate::values::CiphertextArtifact;
use crate::{CheckContext, CheckError, KeyOwnerContext, Result};
use tfhe_math::{EncryptionContext, PublicValue};
use zeroize::Zeroizing;

impl KeyOwnerContext {
    pub fn import_transfer_challenge(
        &self,
        binding: TransferBinding,
        bytes: &[u8],
        digest: crate::Digest,
        expected: &CheckContext,
    ) -> Result<TransferChallenge> {
        TransferChallenge::import(binding, bytes, digest, expected, &self.evaluation)
    }

    /// Local mathematical primitive. The host verifies query authorization/proofs first.
    /// Only the target encryption of z leaves this method; clear z is never returned.
    pub fn reencrypt_masked(
        &self,
        challenge: &TransferChallenge,
        target: &EncryptionContext,
        expected: &CheckContext,
    ) -> Result<TransferResponse> {
        expected.check_id(challenge.binding.context)?;
        if challenge.binding.reference != expected.source.native {
            return Err(CheckError::ReferenceMismatch);
        }
        if self.binding() != expected.source.key || target.binding() != expected.target.key {
            return Err(CheckError::KeyDomainMismatch);
        }

        let masked = Zeroizing::new(
            self.decryption
                .decrypt(&challenge.artifact.value)?
                .as_uint()?,
        );
        let value = PublicValue::uint(expected.width(), *masked)?;
        let artifact = CiphertextArtifact::encrypt(target, value)?;

        Ok(TransferResponse {
            binding: TransferBinding {
                context: expected.id,
                reference: challenge.id,
            },
            artifact,
        })
    }
}
