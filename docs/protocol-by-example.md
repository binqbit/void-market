# Protocol by Example

Void Market

Dave wants to sell a digital asset. John wants to buy it. Void Market’s protocol is designed to answer the questions needed to complete their auction with yes-or-no results, without publishing Dave’s minimum price or John’s bid.

## 1. Dave lists the asset; John submits a bid

Dave creates a listing and sets the lowest price he will accept. John submits the amount he is willing to pay. Their amounts enter the comparison process as encrypted values, tied to this particular auction.

The auction also fixes its deadline, participation rules and tie-breaking rule. Once a bid is committed, subsequent steps refer to that same bid—not a replacement chosen after seeing the outcome.

The automated arbiter coordinates the process: it sends requests, checks responses and records the auction’s progress.

## 2. Comparing everyone with everyone

The comparison stage is all-to-all: buyers’ bids are compared with one another, and bids are compared with the seller’s minimum price. This is not a separate sequence that first chooses a winning buyer and only then checks the seller’s price.

Each comparison asks a specific question, such as “Is John’s bid higher than another buyer’s?” or “Does it meet Dave’s minimum?” Its answer is yes or no. The question, the participants and the encrypted inputs are fixed together, so each response belongs to one particular comparison.

Each buyer is compared with every other buyer and with the seller’s private price.

## 3. Two protections for different stages

The intended protocol uses two different protections. Homomorphic encryption lets the arbiter perform operations and comparisons on encrypted amounts without opening them. One-time masks add protection when a participant must handle an intermediate value outside that encrypted computation, such as during decryption and re-encryption for a comparison or a checking step.

At those boundaries, a fresh secret mask changes the value so that the participant can process it without being shown the original amount. Masking complements encryption; it does not replace it. The transformation is reversible with the corresponding secrets, not irreversible destruction of the value.

Comparison results use a separate masking step. The owners of the compared inputs each contribute a mask to the protected answer, keeping its yes-or-no meaning hidden until the permitted opening. Participants check the requests before responding. Required cryptographic checks link each response to its inputs and operation; masking or signing alone does not prove correct processing.

## 4. Closing the auction and opening results

When bidding closes, the accepted bids are fixed. The protected responses are committed before the masks needed to interpret them are opened. This keeps the answer tied to the same request throughout the exchange.

The arbiter checks the responses and the permitted mask openings. Removing the masks reveals the comparison’s yes-or-no result—not a published price or bid. An answer that cannot be verified remains unresolved rather than becoming a valid result.

The verified comparison results, together with the auction’s rules, identify the winning eligible buyer. If John wins, his payment obligation is his committed bid. If no eligible bid meets Dave’s minimum, there is no sale.

## 5. John pays; Dave accepts

John receives a payment request tied to his winning bid and Dave as the recipient. He checks the request and authorizes a shielded Zcash payment. A submitted payment is tracked through confirmation; an uncertain transaction is checked before another payment is attempted.

Dave can confirm receipt and accept the payment. Once the applicable conditions are satisfied, Void Market’s automated arbiter coordinates delivery of the NFT to John. Ethereum handles the NFT transfer, while Zcash handles the shielded payment. Void Market coordinates these two transfers; they are not an atomic swap.

## 6. If the payment needs checking

If Dave does not accept the payment or reports a shortfall, the dispute process examines the relevant payment outputs. Selective disclosure provides the information needed to verify that payment, without requesting access to John’s entire wallet.

The process checks the recipient, the connection to the invoice, confirmations and the total credited amount. Each payment output counts only once. It then checks whether the verified payments cover John’s committed bid.

If payment is sufficient, no top-up is needed. If a verified shortfall remains, John receives the amount still due and can authorize a top-up. The next check includes confirmed earlier payments, rather than starting the bill again. Missing evidence leaves the dispute unresolved; it is not treated as proof of nonpayment.

## 7. If the purchase cannot finish

If John declines to pay or does not complete a required top-up, the auction follows its agreed rules. Before approaching another eligible buyer, any pending or partial payments must be resolved. The next buyer then follows the same payment process.

If the sale cannot safely continue, it is canceled or held for resolution as appropriate. The asset returns to Dave only when doing so does not conflict with an existing buyer’s entitlement.

The core idea is simple: use protected comparisons to decide the auction, then share the payment information needed to complete that particular trade.
