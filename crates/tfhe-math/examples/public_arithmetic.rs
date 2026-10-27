use tfhe_math::{MathSession, PublicValue, Result, TfheValue, Width};

fn main() -> Result<()> {
    let math = MathSession::public_reference();
    let a = TfheValue::public(PublicValue::uint(Width::U8, 200)?);
    let b = TfheValue::public(PublicValue::uint(Width::U8, 100)?);
    let wide = math.add_wide(&a, &b)?;
    let fixed = math.add_in_width(Width::U8, &a, &b)?;
    println!(
        "public reference: 200 + 100 = {} ({:?}); fixed = {}, overflow = {}",
        wide.public_value()?.as_uint()?,
        wide.data_type(),
        fixed.value.public_value()?.as_uint()?,
        fixed.flag.public_value()?.as_bool()?
    );
    Ok(())
}
