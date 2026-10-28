use market::auctions::{amount, matching};
use uuid::Uuid;

#[test]
fn zec_amounts_are_exact_decimal_strings() {
    for (text, units, normalized) in [
        ("0", 0, "0.00000000"),
        ("1", 100_000_000, "1.00000000"),
        ("1.25", 125_000_000, "1.25000000"),
        ("0.00000001", 1, "0.00000001"),
        (
            "90071992.54740993",
            9_007_199_254_740_993,
            "90071992.54740993",
        ),
        (
            "92233720368.54775807",
            i64::MAX as u64,
            "92233720368.54775807",
        ),
    ] {
        assert_eq!(amount::parse(text).unwrap(), units);
        assert_eq!(amount::format(units), normalized);
    }
    for text in [
        "",
        "-1",
        "+1",
        " 1",
        "1 ",
        "1e3",
        ".5",
        "1.",
        "1,25",
        "1.000000001",
        "92233720368.54775808",
        "18446744073709551615",
        "1.2.3",
    ] {
        assert!(amount::parse(text).is_err(), "{text}");
    }
}

#[test]
fn complete_boolean_transcripts_match_reference_prices_and_ties() {
    let seller = Uuid::from_u128(10);
    let buyers = [Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)];
    for prices in [
        [42, 42, 42],
        [10, 30, 20],
        [30, 10, 20],
        [20, 30, 10],
        [10, 10, 30],
        [0, 1, 2],
    ] {
        for reserve in [0, 10, 20, 30, 42, 43] {
            let slots = matching::schedule(seller, &buyers);
            let results: Vec<bool> = slots
                .iter()
                .map(|slot| {
                    let left = buyers.iter().position(|buyer| *buyer == slot.left).unwrap();
                    if slot.kind == "reserve" {
                        prices[left] >= reserve
                    } else {
                        let right = buyers
                            .iter()
                            .position(|buyer| *buyer == slot.right)
                            .unwrap();
                        prices[left] > prices[right]
                    }
                })
                .collect();
            let expected = prices
                .iter()
                .enumerate()
                .filter(|(_, price)| **price >= reserve)
                .max_by(|(left_index, left), (right_index, right)| {
                    left.cmp(right).then_with(|| right_index.cmp(left_index))
                })
                .map(|(index, _)| buyers[index]);
            assert_eq!(
                matching::winner(seller, &buyers, &results).unwrap(),
                expected
            );
        }
    }
}

#[test]
fn incomplete_cycles_and_nonmonotone_reserve_checks_cannot_award() {
    let seller = Uuid::from_u128(10);
    let buyers = [Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)];
    assert!(matching::winner(seller, &buyers, &[true]).is_err());
    // 2 beats 1, 1 beats 3, 3 beats 2 forms a cycle.
    assert!(matching::winner(seller, &buyers, &[true, false, true, true, true, true]).is_err());
    // 3 > 2 > 1, yet 3 fails the reserve and 2 passes it.
    assert!(matching::winner(seller, &buyers, &[true, true, true, false, true, false]).is_err());
    assert_eq!(
        matching::winner(seller, &buyers[..1], &[true]).unwrap(),
        Some(buyers[0])
    );
    assert_eq!(
        matching::winner(seller, &buyers[..1], &[false]).unwrap(),
        None
    );
    assert_eq!(matching::winner(seller, &[], &[]).unwrap(), None);
}
