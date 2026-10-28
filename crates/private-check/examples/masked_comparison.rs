use private_check::{
    CheckContext, CheckSession, Comparison, ComputationContext, KeyOwnerContext, NumberInput,
    OwnerId, PreparedOwnerMask, Result,
};
use tfhe_math::{KeySet, MathParameters, Width};

fn sample_owner(last: u8) -> Result<OwnerId> {
    let mut bytes = [0; 16];
    bytes[6] = 0x40;
    bytes[8] = 0x80;
    bytes[15] = last;
    OwnerId::new(bytes)
}

fn main() -> Result<()> {
    // Known synthetic inputs and disposable keys for a local mathematical example.
    let source = KeySet::generate(MathParameters)?;
    let target = KeySet::generate(MathParameters)?;
    let x = NumberInput::encrypt(&source.encryption_context(), Width::U8, 42)?;
    let v = NumberInput::encrypt(&target.encryption_context(), Width::U8, 17)?;
    let owners = [sample_owner(1)?, sample_owner(2)?];
    let context = CheckContext::new([1; 32], owners, Comparison::Greater, &x, &v)?;

    let computation = CheckSession::new(
        context,
        ComputationContext::new(source.encryption_context(), source.evaluation_context())?,
        ComputationContext::new(target.encryption_context(), target.evaluation_context())?,
    )?;
    let source_owner =
        KeyOwnerContext::new(source.decryption_context(), source.evaluation_context())?;
    let target_owner =
        KeyOwnerContext::new(target.decryption_context(), target.evaluation_context())?;

    let transfer = computation.prepare_number_transfer(&x)?;
    let response = source_owner.reencrypt_masked(
        transfer.challenge(),
        &target.encryption_context(),
        computation.context(),
    )?;
    let transferred = computation.finish_number_transfer(transfer, response)?;

    let base = computation.prepare_boolean_masks()?;
    let first = PreparedOwnerMask::generate(
        owners[0],
        computation.context(),
        base.commitment(),
        &target.encryption_context(),
    )?;
    let second = PreparedOwnerMask::generate(
        owners[1],
        computation.context(),
        base.commitment(),
        &target.encryption_context(),
    )?;
    let comparison = computation.compare_protected(
        &transferred,
        &v,
        base,
        first.contribution(),
        second.contribution(),
    )?;

    // A host protocol supplies proof, consent and release gates around these primitives.
    let pending = target_owner.prepare_boolean_answer(
        comparison.challenge(),
        computation.context(),
        comparison.base_commitment(),
    )?;
    let commitment = pending.commitment();
    let answer = pending.reveal(comparison.base_commitment())?;
    let decoded =
        computation.decode_boolean(comparison, commitment, answer, first.open(), second.open())?;
    println!(
        "synthetic mathematical check: 42 > 17 -> {}",
        decoded.value()
    );

    Ok(())
}
