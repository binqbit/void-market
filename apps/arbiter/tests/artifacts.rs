use arbiter::{
    artifacts::Artifacts,
    crypto::{Assurance, CheckManifest, ComparisonEngine, KeyMaterial, Predicate},
    error::Error,
};
use arbiter_protocol::ArtifactRef;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
use uuid::Uuid;

fn folder() -> PathBuf {
    let root = std::env::var_os("TEST_ARTIFACT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn immutable_artifacts_detect_tampering_and_hide_small_value_hashes() {
    let root = folder();
    let store = Artifacts::open(&root).unwrap();
    let first = store.write_json(&true).unwrap();
    let second = store.write_json(&true).unwrap();
    assert_ne!(first.digest, second.digest);
    assert!(store.record::<bool>(&first).unwrap());
    assert!(store.read(&first, 1).is_err());
    fs::write(root.join(format!("{}.bin", first.id)), b"tampered").unwrap();
    assert!(store.record::<bool>(&first).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn proof_required_does_not_import_keys_or_start_a_challenge() {
    let root = folder();
    let store = Artifacts::open(&root).unwrap();
    let missing = ArtifactRef {
        id: Uuid::new_v4(),
        digest: [0; 32],
    };
    let keys = KeyMaterial {
        configuration: [0; 32],
        public_fingerprint: [0; 32],
        evaluation_fingerprint: [0; 32],
        public_key: missing.clone(),
        evaluation_key: missing.clone(),
    };
    let manifest = CheckManifest {
        release_at_ms: None,
        schema_version: 1,
        assurance: Assurance::ProofRequired,
        external_context: [0; 32],
        owners: [Uuid::new_v4(), Uuid::new_v4()],
        width: 8,
        comparison: Predicate::Greater,
        keys: [keys.clone(), keys],
        inputs: [missing.clone(), missing],
    };
    assert!(matches!(
        ComparisonEngine::start(store, manifest),
        Err(Error::ProofUnavailable)
    ));
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn identity_creation_preserves_existing_keys() {
    let root = folder();
    let directory = root.join("identity");
    let node = arbiter::identity::initialize(&directory, Uuid::new_v4()).unwrap();
    assert!(!node.is_nil());
    let key = fs::read(directory.join("signing.key")).unwrap();
    assert_eq!(key.len(), 32);
    assert_eq!(
        fs::metadata(directory.join("signing.key"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(arbiter::identity::initialize(&directory, Uuid::new_v4()).is_err());
    assert_eq!(fs::read(directory.join("signing.key")).unwrap(), key);
    fs::remove_dir_all(root).unwrap();
}
