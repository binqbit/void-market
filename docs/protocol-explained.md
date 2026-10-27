# Plain-language explanation

Void Market aims to compare private bids without exposing their amounts. This guide explains the building blocks for readers new to the protocol. The [cryptographic protocol](cryptographic-protocol.md) gives the mathematical detail.

**Scope:** these building blocks are not yet a complete, proven secure comparison protocol. The arbiter is an automated program. A required privacy boundary is that **one participant's bid must not be placed under another participant's personal decryption key**. That key owner could read it if they obtained the encrypted data. The previously illustrated personal-key conversion is therefore not an accepted safe solution; the underlying comparison construction remains unresolved.

## 1. Hiding a number

**Purpose:** conceal an original amount during an authorized operation.

The input belongs to the user. The party performing the permitted masking operation owns its fresh secret masks. Add a secret offset, then apply a secret XOR mask. XOR is a reversible bit operation: applying the same mask twice removes it.

```text
Original number: 12    Offset: 5    XOR mask: 9
Hide:    (12 + 5) XOR 9 = 24
Recover: (24 XOR 9) - 5 = 12
```

These small values make the arithmetic readable; they do not demonstrate security strength. For the 64-bit profile, the number and offset each use at most 63 bits, while the XOR mask uses all 64 bits. The sum fits without overflow.

**Masking is not encryption.** A masked number is still an ordinary number. Encryption produces data that requires a secret key to read. Masking can also be computed *inside* encryption, without opening the original value.

With an independent, uniformly random secret XOR mask, the output conceals the original from an observer without that mask. It does **not** prove that a returned number is correct: a replacement may still decode to an allowed number.

Numeric masks are never disclosed in the final audit. Giving someone both a masked number and every mask needed to recover it would reveal the original.

[Technical detail: numeric masking](cryptographic-protocol.md#numeric-masking-hide-the-original-number).

## 2. Hiding a yes-or-no answer

**Purpose:** prevent a comparison result from being read before its authorized release.

The input is one bit: “no” or “yes.” The encoding party chooses two distinct numerical labels, then hides their positions with XOR.

```text
Offset: 10    Distance between labels: 7    XOR mask: 6
“No”:  label 10.       Hidden answer: 10 XOR 6 = 12.
“Yes”: label 10 + 7.   Hidden answer: 17 XOR 6 = 23.
```

To decode, remove XOR and accept only 10 or 17. Any other value is invalid—not an approximate answer. The distance must be nonzero so the labels cannot coincide.

In the actual width rule, the distance and offset each use one bit less than the result width; XOR uses the full width. Smaller demonstration modes preserve the idea, not a production security guarantee.

There are **two separate checks**: “Is this a valid answer label?” and “Is this the correct result of the authorized comparison?” Decoding answers only the first. A separate proof must establish the second.

[Technical detail: Boolean encoding and decoding](cryptographic-protocol.md#boolean-masking-encode-two-distinct-outcomes).

## 3. Checking the same number under two keys

**Purpose:** establish that changing an encrypted representation did not change its content. The value could be a permitted mask or another number.

Imagine two closed boxes with different locks. The required proof says their contents match, without showing those contents to the checker. It must refer to the exact boxes, registered keys and request—not unrelated examples. Matching contents do not imply matching encrypted bytes.

This is a **statement to prove, not permission to transfer any data to any key**. In particular, encrypting a foreign bid under a competitor's personal key violates the privacy boundary above. Proving equality does not remove that problem. A safe use depends on the eventual key architecture; no replacement construction has been selected here.

If a participant adds an extra private mask during conversion, the two contents are no longer identical. The required statement becomes “the second content is the specified transformation of the first.” How to remove that mask or continue computing correctly must also be defined. This remains a separate open design question.

[Technical detail: key ownership and its limits](cryptographic-protocol.md#key-topology-and-privacy-scope).

## 4. Keeping the result hidden from the arbiter

The arbiter knows its own yes/no encoding masks. Those masks alone cannot hide the answer **from the arbiter itself**.

Each user therefore contributes a separate secret **output mask**, covering the entire result width:

```text
Protected answer = encoded answer XOR User 1's mask XOR User 2's mask
```

The intended hiding argument requires at least one mask to remain unknown to the arbiter, fresh, independent and uniformly random across the full width. No other message or intermediate may reveal the unprotected answer or missing mask. Knowing every required mask—or obtaining an unprotected intermediate—breaks that argument.

Each user must prove knowledge of their own mask and its connection to the actual computation, without revealing it early. Knowing a mask is not proof that it was sampled randomly. The concrete proof construction remains to be established.

At the authorized release point, users disclose only the permitted output masks and matching opening information—not their entire private state or secret keys. Numeric masks remain private. The arbiter's Boolean encoding masks have a separate later audit gate.

[Technical detail: owner masks and disclosure gates](cryptographic-protocol.md#owner-masks-and-the-two-disclosure-gates).

## 5. Fixing an answer is not proving it correct

A participant first records a hash of the protected answer and its request context. Later, the answer must match that hash.

- **Hash:** binds the recorded answer against replacement, under the hash function's required properties.
- **Proof:** establishes that this answer came from the exact encrypted output or authorized calculation.
- **Signature:** identifies the approving key; it does not establish correct arithmetic.

A hash alone does not hide a value. A small set of possible answers can be searched. None of these mechanisms replaces the others.

Likewise, a “correct number” means consistency with the committed input and its allowed range—not proof of a bank balance, available funds or completed payment.

[Technical detail: required evidence](cryptographic-protocol.md#what-must-be-proved).

## 6. How the pieces fit together

1. **Bind the bids:** establish that each encrypted input matches its commitment and allowed range.
2. **Freeze the request:** fix the bids, participants, profile and comparison rule before releasing results.
3. **Compare privately:** preserve the prohibition on foreign bids under participants' personal keys. **This underlying construction is still unresolved.**
4. **Protect the answer:** apply the Boolean encoding and users' output masks, with evidence linking the exact inputs and mask contributions.
5. **Fix and verify the response:** the hash binds it; the required proofs establish its relationship to the authorized operation. Missing evidence means no verified result.
6. **Open only authorized output masks:** obtain the permitted result for winner selection, without disclosing bid amounts.
7. **Audit later:** after the required completion conditions, allow a scoped Boolean audit. Never include numeric recovery masks.

| Reader | Intended knowledge | Must not receive |
|---|---|---|
| Each user | Their own bid, keys and masks | A foreign bid encrypted under their personal key |
| Automated arbiter | Authorized context, its own masks and required verification data | Open bids; prematurely, everything needed to read the result |
| Authorized result recipient | The permitted outcome after its release gate | Automatic access to amounts or every secret |

[Technical detail: comparison sequence](cryptographic-protocol.md#one-comparison-from-inputs-to-result).

**In one sentence:** masks hide values, commitments and hashes fix choices, proofs establish relationships, and disclosure rules control who learns the result and when. The arithmetic explains the building blocks; it does not by itself complete or validate the whole protocol.

At this stage of protocol development, we cannot yet guarantee complete privacy. Further research and protocol improvements are planned to strengthen its privacy guarantees. The construction and verification requirements remain open; this explanation does not claim a completed privacy guarantee.
