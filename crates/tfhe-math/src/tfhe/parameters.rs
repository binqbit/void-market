use crate::{MathError, Result};

pub(super) fn config() -> ::tfhe::Config {
    ::tfhe::ConfigBuilder::with_custom_parameters(
        ::tfhe::shortint::parameters::PARAM_MESSAGE_2_CARRY_2_KS_PBS_TUNIFORM_2M128,
    )
    .use_dedicated_oprf_key(false)
    .build()
}

pub(crate) fn canonical_config_bytes() -> Result<Vec<u8>> {
    // Only a trusted, locally constructed Config is serialized here.
    bincode::serialize(&config()).map_err(|_| MathError::InvalidEncoding)
}
