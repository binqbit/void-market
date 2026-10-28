use crate::http::error::{ApiError, Result};
use check_workflow::messages::Predicate;
use uuid::Uuid;

pub struct Slot {
    pub kind: &'static str,
    pub left: Uuid,
    pub right: Uuid,
    pub predicate: Predicate,
}

/// Buyer order is immutable admission order. A false later>earlier result preserves ties.
pub fn schedule(seller: Uuid, buyers: &[Uuid]) -> Vec<Slot> {
    let mut slots = Vec::new();
    for (earlier, right) in buyers.iter().enumerate() {
        for left in &buyers[earlier + 1..] {
            slots.push(Slot {
                kind: "pair",
                left: *left,
                right: *right,
                predicate: Predicate::Greater,
            });
        }
    }
    for buyer in buyers {
        slots.push(Slot {
            kind: "reserve",
            left: *buyer,
            right: seller,
            predicate: Predicate::GreaterOrEqual,
        });
    }
    slots
}

/// Validate the complete Boolean transcript; never sort or decrypt bid amounts.
pub fn winner(seller: Uuid, buyers: &[Uuid], results: &[bool]) -> Result<Option<Uuid>> {
    let slots = schedule(seller, buyers);
    if slots.len() != results.len() {
        return Err(ApiError::Conflict("Incomplete comparison transcript"));
    }
    if buyers.is_empty() {
        return Ok(None);
    }
    let mut ranked: Vec<(Uuid, usize)> = buyers.iter().map(|buyer| (*buyer, 0)).collect();
    let mut edges = Vec::new();
    for (slot, value) in slots.iter().zip(results) {
        if slot.kind == "pair" {
            let (winner, loser) = if *value {
                (slot.left, slot.right)
            } else {
                (slot.right, slot.left)
            };
            ranked
                .iter_mut()
                .find(|(buyer, _)| *buyer == winner)
                .ok_or(ApiError::Internal)?
                .1 += 1;
            edges.push((winner, loser));
        }
    }
    ranked.sort_by_key(|(_, wins)| std::cmp::Reverse(*wins));
    for (position, (_, wins)) in ranked.iter().enumerate() {
        if *wins != buyers.len() - position - 1 {
            return Err(ApiError::Conflict("Inconsistent comparison transcript"));
        }
    }
    for (winner, loser) in edges {
        let winner_rank = ranked
            .iter()
            .position(|(buyer, _)| *buyer == winner)
            .ok_or(ApiError::Internal)?;
        let loser_rank = ranked
            .iter()
            .position(|(buyer, _)| *buyer == loser)
            .ok_or(ApiError::Internal)?;
        if winner_rank >= loser_rank {
            return Err(ApiError::Conflict("Inconsistent comparison transcript"));
        }
    }

    let mut below_reserve = false;
    let mut selected = None;
    for (buyer, _) in ranked {
        let qualified = slots
            .iter()
            .zip(results)
            .find(|(slot, _)| slot.kind == "reserve" && slot.left == buyer)
            .map(|(_, qualified)| *qualified)
            .ok_or(ApiError::Internal)?;
        if qualified && below_reserve {
            return Err(ApiError::Conflict("Inconsistent reserve eligibility"));
        }
        if !qualified {
            below_reserve = true;
        }
        if qualified && selected.is_none() {
            selected = Some(buyer);
        }
    }
    Ok(selected)
}
