mod support;

use arbiter::{
    artifacts::Artifacts,
    crypto::{ComparisonEngine, Predicate, Progress, Request, Response},
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
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
    for (comparison, expected, release_at) in [
        (Predicate::Greater, true, None),
        (Predicate::Less, false, None),
        (Predicate::Greater, true, Some(0)),
    ] {
        let mut manifest = prepared.clone();
        manifest.comparison = comparison;
        manifest.release_at_ms = release_at;
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
                    assert_eq!(steps, 4);
                    break;
                }
            }
        }
    }

    let mut guarded = prepared.clone();
    guarded.release_at_ms = Some(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
            + 60_000,
    );
    let (mut guarded_engine, Progress::Waiting(reference)) =
        ComparisonEngine::start(store.clone(), guarded).unwrap()
    else {
        panic!("An auction must request early masks before requesting decryption");
    };
    let request: Request = store.record(&reference).unwrap();
    assert!(matches!(request, Request::Preparation { .. }));
    let mut participants = support::Participants::new(&keys, store.clone());
    assert!(
        guarded_engine
            .advance(participants.respond(request))
            .is_err(),
        "The node must reject completing preparation before the release gate"
    );

    #[cfg(feature = "demo-mode")]
    {
        let mut accelerated = prepared.clone();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        accelerated.release_at_ms = Some(now + 3_600_000);
        let (mut engine, Progress::Waiting(reference)) =
            ComparisonEngine::start(store.clone(), accelerated).unwrap()
        else {
            panic!("A demo auction must collect masks before early closure");
        };
        let request: Request = store.record(&reference).unwrap();
        let mut participants = support::Participants::new(&keys, store.clone());
        let collected_masks = participants.respond(request);
        let mut progress = engine
            .advance_after_demo_close(collected_masks, now)
            .unwrap();
        let mut steps = 1;
        loop {
            match progress {
                Progress::Waiting(reference) => {
                    let request: Request = store.record(&reference).unwrap();
                    progress = engine.advance(participants.respond(request)).unwrap();
                    steps += 1;
                }
                Progress::Complete(output) => {
                    let result: serde_json::Value = store.record(&output).unwrap();
                    assert_eq!(result["value"], true);
                    assert_eq!(steps, 4, "Early closure must reuse the collected masks");
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
