use private_check::{
    BooleanChallenge, CheckContext, CheckSession, Comparison, ComputationContext, DecodedBoolean,
    Digest, KeyOwnerContext, NumberInput, OwnerId, PreparedComparison, PreparedOwnerMask,
    TransferredNumber,
};
use std::sync::OnceLock;
use tfhe_math::{
    DataType, KeySet, MathParameters, Width,
    codec::{EncodedObject, EncodingLimits, import_ciphertext},
};

pub fn source_keys() -> &'static KeySet {
    static KEYS: OnceLock<KeySet> = OnceLock::new();
    KEYS.get_or_init(|| KeySet::generate(MathParameters).unwrap())
}

pub fn target_keys() -> &'static KeySet {
    static KEYS: OnceLock<KeySet> = OnceLock::new();
    KEYS.get_or_init(|| KeySet::generate(MathParameters).unwrap())
}

pub fn owners() -> [OwnerId; 2] {
    let mut first = [1; 16];
    first[6] = 0x41;
    first[8] = 0x81;
    let mut second = first;
    second[15] = 2;
    [OwnerId::new(first).unwrap(), OwnerId::new(second).unwrap()]
}

pub fn source_owner() -> KeyOwnerContext {
    KeyOwnerContext::new(
        source_keys().decryption_context(),
        source_keys().evaluation_context(),
    )
    .unwrap()
}

pub fn target_owner() -> KeyOwnerContext {
    KeyOwnerContext::new(
        target_keys().decryption_context(),
        target_keys().evaluation_context(),
    )
    .unwrap()
}

pub struct Fixture {
    pub source: NumberInput,
    pub target: NumberInput,
    pub session: CheckSession,
}

impl Fixture {
    pub fn new(width: Width, x: u64, v: u64, predicate: Comparison) -> Self {
        let source = NumberInput::encrypt(&source_keys().encryption_context(), width, x).unwrap();
        let target = NumberInput::encrypt(&target_keys().encryption_context(), width, v).unwrap();
        let context = CheckContext::new([9; 32], owners(), predicate, &source, &target).unwrap();
        let session = CheckSession::new(
            context,
            ComputationContext::new(
                source_keys().encryption_context(),
                source_keys().evaluation_context(),
            )
            .unwrap(),
            ComputationContext::new(
                target_keys().encryption_context(),
                target_keys().evaluation_context(),
            )
            .unwrap(),
        )
        .unwrap();
        Self {
            source,
            target,
            session,
        }
    }

    pub fn context(&self) -> &CheckContext {
        self.session.context()
    }

    pub fn transfer(&self) -> TransferredNumber {
        let transfer = self.session.prepare_number_transfer(&self.source).unwrap();
        assert!(flag(transfer.range_check().encoded(), source_keys()));
        assert!(!flag(transfer.addition_overflow(), source_keys()));

        let challenge = source_owner()
            .import_transfer_challenge(
                transfer.challenge().binding(),
                transfer.challenge().encoded().as_bytes(),
                transfer.challenge().encoded().digest(),
                self.context(),
            )
            .unwrap();
        let returned = source_owner()
            .reencrypt_masked(
                &challenge,
                &target_keys().encryption_context(),
                self.context(),
            )
            .unwrap();
        let imported = private_check::TransferResponse::import(
            returned.binding(),
            returned.encoded().as_bytes(),
            returned.encoded().digest(),
            transfer.challenge(),
            self.context(),
            &target_keys().evaluation_context(),
        )
        .unwrap();
        let transferred = self
            .session
            .finish_number_transfer(transfer, imported)
            .unwrap();
        assert!(!flag(transferred.subtraction_borrow(), target_keys()));
        transferred
    }

    pub fn comparison(&self) -> (PreparedComparison, PreparedOwnerMask, PreparedOwnerMask) {
        let transferred = self.transfer();
        let base = self.session.prepare_boolean_masks().unwrap();
        let first = PreparedOwnerMask::generate(
            owners()[0],
            self.context(),
            base.commitment(),
            &target_keys().encryption_context(),
        )
        .unwrap();
        let second = PreparedOwnerMask::generate(
            owners()[1],
            self.context(),
            base.commitment(),
            &target_keys().encryption_context(),
        )
        .unwrap();
        let prepared = self
            .session
            .compare_protected(
                &transferred,
                &self.target,
                base,
                second.contribution(),
                first.contribution(),
            )
            .unwrap();
        for check in prepared.arithmetic_checks() {
            assert!(!flag(check, target_keys()));
        }
        for check in prepared.number_checks() {
            assert!(flag(check, target_keys()));
        }
        (prepared, first, second)
    }

    pub fn decode(&self) -> DecodedBoolean {
        let (prepared, first, second) = self.comparison();
        let challenge = import_boolean(
            prepared.challenge(),
            self.context(),
            prepared.base_commitment(),
        );
        let pending = target_owner()
            .prepare_boolean_answer(&challenge, self.context(), prepared.base_commitment())
            .unwrap();
        let commitment = pending.commitment();
        let answer = pending.reveal(prepared.base_commitment()).unwrap();
        self.session
            .decode_boolean(prepared, commitment, answer, second.open(), first.open())
            .unwrap()
    }
}

pub fn import_boolean(
    challenge: &BooleanChallenge,
    expected: &CheckContext,
    base: Digest,
) -> BooleanChallenge {
    target_owner()
        .import_boolean_challenge(
            challenge.binding(),
            challenge.encoded().as_bytes(),
            challenge.encoded().digest(),
            expected,
            base,
        )
        .unwrap()
}

pub fn flag(encoded: &EncodedObject, keys: &KeySet) -> bool {
    let value = import_ciphertext(
        encoded.as_bytes(),
        encoded.digest(),
        DataType::Bool,
        &keys.evaluation_context(),
        EncodingLimits::default(),
    )
    .unwrap();
    keys.decryption_context()
        .decrypt(&value)
        .unwrap()
        .as_bool()
        .unwrap()
}
