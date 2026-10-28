mod support;

use arbiter::{
    artifacts::Artifacts,
    crypto::{ComparisonEngine, Predicate, Progress, Request, Response},
};
use std::{fs, path::PathBuf};
use tfhe_math::{KeySet, MathParameters, Width};
use uuid::Uuid;

#[test]
#[ignore = "real TFHE key generation and encrypted comparison"]
fn independent_owner_roundtrip_uses_the_node_engine() {
    let root = std::env::var_os("TEST_ARTIFACT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(Uuid::new_v4().to_string());
    let store = Artifacts::open(&root).unwrap();
    let keys = [
        KeySet::generate(MathParameters).unwrap(),
        KeySet::generate(MathParameters).unwrap(),
    ];
    let prepared = support::prepare(&store, &keys, [42, 17], Width::U8, Predicate::Greater);
    for (comparison, expected) in [(Predicate::Greater, true), (Predicate::Less, false)] {
        let mut manifest = prepared.clone();
        manifest.comparison = comparison;
        let (mut engine, mut progress) = ComparisonEngine::start(store.clone(), manifest).unwrap();
        let mut participants = support::Participants::new(&keys, store.clone());
        let mut steps = 0;
        loop {
            match progress {
                Progress::Waiting(request) => {
                    let request: Request = store.record(&request).unwrap();
                    progress = engine.advance(participants.respond(request)).unwrap();
                    steps += 1;
                }
                Progress::Complete(output) => {
                    let result: serde_json::Value = store.record(&output).unwrap();
                    assert_eq!(result["value"], expected);
                    assert_eq!(result["proof_status"], "unavailable");
                    assert_eq!(steps, 5);
                    break;
                }
            }
        }
    }

    let (mut engine, progress) = ComparisonEngine::start(store.clone(), prepared).unwrap();
    assert!(matches!(progress, Progress::Waiting(_)));
    let premature = Response::AnswerCommitment {
        context: [0; 32],
        challenge: [0; 32],
        commitment: [0; 32],
    };
    assert!(engine.advance(premature).is_err());

    fs::remove_dir_all(root).unwrap();
}
