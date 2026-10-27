use super::parameters;

pub(crate) struct NativeKeys {
    pub client: ::tfhe::ClientKey,
    pub public: ::tfhe::CompactPublicKey,
    pub evaluation: ::tfhe::ServerKey,
}

pub(crate) fn generate() -> NativeKeys {
    let client = ::tfhe::ClientKey::generate(parameters::config());
    let public = ::tfhe::CompactPublicKey::new(&client);
    let evaluation = client.generate_server_key();

    NativeKeys {
        client,
        public,
        evaluation,
    }
}
