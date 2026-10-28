use super::bounded_with;
use crate::CheckError;
use tfhe_math::Width;

#[test]
fn zero_is_rejected_without_bias_or_an_unbounded_loop() {
    let mut calls = 0;
    let sample = bounded_with(Width::U8, true, |buffer| {
        calls += 1;
        buffer.fill(0);
        if calls == 2 {
            buffer[0] = 255;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(*sample, 127);
    assert_eq!(calls, 2);

    let failure = bounded_with(Width::U8, true, |buffer| {
        buffer.fill(0);
        Ok(())
    });
    assert_eq!(failure, Err(CheckError::EntropyUnavailable));
}

#[test]
fn inclusive_bounds_and_entropy_failures_cover_every_width() {
    for width in Width::ALL {
        let maximum = bounded_with(width, false, |buffer| {
            buffer.fill(255);
            Ok(())
        })
        .unwrap();
        assert_eq!(*maximum, width.maximum() / 2);
        let zero = bounded_with(width, false, |buffer| {
            buffer.fill(0);
            Ok(())
        })
        .unwrap();
        assert_eq!(*zero, 0);
    }

    let failure = bounded_with(Width::U8, false, |_| Err(CheckError::EntropyUnavailable));
    assert_eq!(failure, Err(CheckError::EntropyUnavailable));
}
