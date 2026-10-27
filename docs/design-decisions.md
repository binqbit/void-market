# Design decisions

Void Market's design starts with a narrow objective: match an asset to the highest eligible private bid, then coordinate payment and delivery without exposing prices in the ordinary path. The decisions below connect each challenge to the chosen response and its limits.

## Hide bids, not the existence of a sale

**Challenge.** A conventional visible auction reveals price information before participants have finished bidding.

**Chosen response.** Accept encrypted fixed bids, evaluate only the agreed comparisons, and release the authorized outcomes after the bidding freeze. Publish only the sale information appropriate to each audience.

**Why.** Participants cannot use an early comparison result to revise their bid within the same auction. Numeric prices do not need to become a public leaderboard.

**Tradeoff.** The asset identifier, public-chain transfer addresses, timing, and eventual winner may still be observable. A payment dispute can reveal a particular amount to an authorized reviewer. Privacy is a defined information boundary, not a promise that nobody can observe any aspect of the sale.

See [the privacy scope](concept.md#what-remains-private).

## Separate hiding from correctness

**Challenge.** A random-looking number can still be a fabricated answer. Fixing its hash only prevents changing it later.

**Chosen response.** Use numeric masking for hiding, spaced Boolean encoding for the comparison output, and distinct proof requirements for input validity, authorized evaluation, and correct decryption.

**Why.** Each mechanism has one clear responsibility. Exact endpoint checking rejects many malformed answers, while the proof requirements bind an accepted endpoint to the computation that was actually authorized.

**Tradeoff.** A complete strong-verification path is more demanding than mask arithmetic. If a required proof capability is absent, the design cannot honestly claim that a signed demonstration response is a cryptographically verified comparison.

See [the formulas](cryptographic-protocol.md#boolean-masking-encode-two-distinct-outcomes) and [proof obligations](cryptographic-protocol.md#what-must-be-proved).

## Use distinct disclosure stages

**Challenge.** The arbiter needs comparison results to select a winner, but early access can leak competitive information. Delaying every secret until after payment makes winner selection impossible.

**Chosen response.** Open owner output masks after frozen bidding and a fixed authorized answer; open Boolean audit parameters only after accepted payment and completed operations. Never open numeric transfer masks for the final audit.

**Why.** Winner selection and retrospective audit have different information needs. The two gates prevent a circular payment dependency and avoid treating audit as permission to disclose all bid-related secrets.

**Tradeoff.** A participant must remain able to provide a valid required opening. Refusal and lost-state behavior need explicit resolution rules; a later audit cannot undo an early disclosure.

See [owner masks and disclosure gates](cryptographic-protocol.md#owner-masks-and-the-two-disclosure-gates).

## Fix comparisons instead of creating a probing interface

**Challenge.** Even encrypted comparisons can reveal a number if an evaluator repeatedly asks whether it exceeds chosen thresholds. A visible winner-only bracket can also expose information through its shape.

**Chosen response.** Fix the allowed input set, pair schedule, predicates, and disclosure scope. Use the required buyer comparisons and reserve checks, with immutable tie order and no arbitrary threshold requests.

**Why.** The transcript serves the auction's one authorized decision rather than becoming a general-purpose oracle about participants' prices.

**Tradeoff.** A fixed complete schedule may require more comparisons. One successful pair is not enough to establish the global winner; unfinished required comparisons remain unresolved.

See [comparison preparation](how-it-works.md#5-prepare-comparisons-without-revealing-their-answers) and [winner selection](how-it-works.md#7-determine-the-winner-and-issue-the-payment-request).

## Start with an explicit key boundary

**Challenge.** Comparing ciphertexts from unrelated keys is not automatically provided by an arithmetic wrapper, and one party's decryption capability creates a real privacy boundary.

**Chosen response.** Keep the common auction key and seller-as-decryptor arrangement as a retained matching blueprint, not an accepted safe solution to the requirement that another participant must not gain a decryptable foreign bid. The final matching key construction remains unresolved. Preserve owner-result masks and pre-decryption verification while that construction is specified.

**Why.** A concrete earlier blueprint helps identify the key-ownership problem; it does not resolve the stronger privacy requirement. No multi-key, threshold or other replacement construction is adopted by this description.

**Tradeoff.** At this stage of protocol development, we cannot yet guarantee complete privacy. Further research and protocol improvements are planned to strengthen its privacy guarantees. The matching key construction and its verification requirements remain open; no replacement construction is adopted by this documentation.

See [key topology](cryptographic-protocol.md#key-topology-and-privacy-scope).

## Keep automated arbitration and custody visible

**Challenge.** The platform must coordinate the sale and control delivery while making the service responsibility and seller’s asset-transfer consent explicit.

**Chosen response.** Use a platform-owned automated arbiter as a separate internal service. Authenticate its registration and listing claim, fix the assignment, and require seller approval of the exact custody and transfer terms. Confirm custody of the exact asset before progressing.

**Why.** Responsibility is explicit before the seller relinquishes control. The internal service may handle several sales, while each retains its own versioned assignment and consent.

**Tradeoff.** The selected direct-custody demonstration still depends on the arbiter. Separate internal services do not by themselves establish correct execution or recovery. Worker balancing must preserve custody authority and the seller’s approved transfer terms.

See [approval](how-it-works.md#1-create-a-listing-and-approve-custody-terms) and [custody](how-it-works.md#2-establish-confirmed-asset-custody).

## Do not call coordinated settlement atomic

**Challenge.** The asset and private payment live on different networks. A confirmed payment does not automatically transfer an NFT.

**Chosen response.** Coordinate confirmed custody, accepted payment, and confirmed asset delivery as separate guarded stages. Preserve recovery and safe-cancellation paths.

**Why.** This keeps the first demonstration focused on private matching while accurately representing the remaining delivery responsibility.

**Tradeoff.** A failure can occur between payment and asset delivery. The platform and arbiter retain responsibilities that an atomic exchange would otherwise enforce. Returning an asset must wait if payment, a dispute, or an outgoing transfer remains unresolved.

See [delivery and cancellation](how-it-works.md#9-deliver-the-asset-or-safely-cancel).

## Use scoped payment evidence rather than broad wallet disclosure

**Challenge.** A transaction identifier alone does not identify a hidden recipient or prove who funded the payment. Asking the recipient to prove they own no relevant wallet is not a workable dispute rule.

**Chosen response.** Use a sale-bound payment request, ordinary recipient acceptance, and a dispute path tied to specific confirmed outputs. The selected dispute design compares the cumulative verified paid total with the buyer’s own frozen bid under the buyer’s own key. Only a sound underpayment result permits the separate encrypted difference and top-up, using the identical bid and credit snapshot; changed credits require a new comparison. Each eligible output counts once. Concrete proof/key implementation remains open, and no automatic seller fine follows a rejected shortfall claim.

**Why.** The process examines the payment relevant to this sale rather than requesting a complete wallet history or treating an unsubstantiated denial as decisive.

**Tradeoff.** Ordinary acceptance is optimistic, not a hidden-amount equality proof. The selected dispute disclosure can reveal the payment amount to its reviewer. Fully private amount-binding proof remains a distinct stronger capability, not something a receipt silently supplies.

See [payment confirmation and disputes](how-it-works.md#8-confirm-payment-or-resolve-a-scoped-dispute).

## Make asynchronous progress attributable

**Challenge.** Participants, the market, and the arbiter may respond at different times. Message retries and a slow verifier can unfairly charge or duplicate somebody else's action.

**Chosen response.** Represent work as explicit requests and state transitions, with deliberate participant approvals, duplicate-safe processing, and cumulative per-role response budgets.

**Why.** The party holding a valid obligation is responsible for the next action. Independent work can progress concurrently without pretending all parties are continuously online.

**Tradeoff.** Recovery must distinguish a real refusal from an invalid request, unavailable proof capability, or unknown external transaction state. Expiration is not automatic permission to punish a participant or transfer an asset.

See [asynchronous requests](how-it-works.md#4-handle-requests-asynchronously).

## Separate mathematical operations from market policy

**Challenge.** A single component that mixes arithmetic, secret handling, message coordination, and sale rules becomes difficult to reason about.

**Chosen response.** Separate abstract arithmetic and Boolean operations, private-operation coordination, market rules, and external payment or asset integrations. Public and encrypted computation share understandable value semantics without permitting an implicit decryption shortcut.

**Why.** A type conversion should not change an auction rule; a transport event should not certify a proof; a sale status should not be mistaken for a network transaction.

**Tradeoff.** Component boundaries need explicit contracts and consistent error behavior. Smaller test widths help exercise behavior but do not establish the security or runtime performance of the live target. Production-width boundaries still require their own verification.

See [conceptual components](concept.md#how-the-pieces-fit-together) and [widths](cryptographic-protocol.md#values-widths-and-exact-arithmetic).

## Evidence before stronger claims

The selected design preserves three distinctions throughout:

1. A documented mathematical identity is not a proof of the complete interactive protocol.
2. A demonstrated workflow is not an independently verified cryptographic execution.
3. A confirmed action on one network is not completion of the whole sale.

These distinctions define what the product must show as it advances. They do not change the core goal: private matching with understandable responsibilities, selective disclosure, and evidence for every accepted transition.
