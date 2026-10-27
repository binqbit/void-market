use tfhe_math::{
    DataType, KeySet, MathParameters, MathSession, PublicValue, Result, Width,
    codec::{EncodingLimits, export_ciphertext, import_ciphertext},
};

fn main() -> Result<()> {
    let keys = KeySet::generate(MathParameters)?;
    let encrypt = keys.encryption_context();
    let math = MathSession::encrypted(keys.evaluation_context());
    let decrypt = keys.decryption_context();
    let a = encrypt.encrypt(PublicValue::uint(Width::U8, 200)?)?;
    let b = encrypt.encrypt(PublicValue::uint(Width::U8, 100)?)?;
    let wide = math.add_wide(&a, &b)?;
    let fixed = math.add_in_width(Width::U8, &a, &b)?;
    let artifact = export_ciphertext(&wide, EncodingLimits::default())?;
    let restored = import_ciphertext(
        artifact.as_bytes(),
        artifact.digest(),
        DataType::Uint(Width::U16),
        &keys.evaluation_context(),
        EncodingLimits::default(),
    )?;
    // This example explicitly acts as the secret-key owner. Applications supply
    // their own authenticated artifact registry and authorization before reveal.
    println!(
        "native TFHE: 200 + 100 = {}; fixed = {}, overflow = {}; artifact bytes = {}",
        decrypt.decrypt(&restored)?.as_uint()?,
        decrypt.decrypt(&fixed.value)?.as_uint()?,
        decrypt.decrypt(&fixed.flag)?.as_bool()?,
        artifact.as_bytes().len()
    );
    Ok(())
}
