# Demo guide

The local demo exercises the marketplace with real TFHE computations, Ethereum transactions on Anvil and shielded Zcash transactions on Zebra/Zallet regtest. Accounts, funds and NFTs belong to these local networks and have no real monetary value. The market holds user keys and receives entered amounts; this demonstrates the workflow, not isolation from the platform or complete malicious-party proof verification.

## Start and choose an account

Start the services with `python3 scripts/localnet/compose.py up` and open **http://localhost:3100/demo**. Use **Alice**, **Bob** or **Daniel** to sign in without Google. Switching accounts preserves each account's own assets, bids and messages.

Use **Claim 10 ZEC** to fund a buyer and **Get NFT** to obtain an asset. Wait for the transaction to confirm; a submitted request is not yet an updated balance or a received NFT.

## Complete a sale

1. Sign in as **Alice** and get a local NFT.
2. Open My portfolio and choose **Sell asset**. Set the reserve to **1.0 ZEC** and choose an end time more than one hour ahead. Review and publish the draft.
3. In Messages, approve the separate NFT custody transaction. The auction opens after the NFT reaches the assigned arbiter's Ethereum address.
4. Sign in as **Bob**, claim local ZEC and bid **2.0 ZEC** in Market.
5. Sign in as **Daniel**, claim local ZEC and bid **3.0 ZEC**. Leave enough balance for the payment and its displayed fee.
6. Review each account's Messages and approve available protocol requests. Alice can open the auction from My portfolio and select **Close bidding now** to start the final phase.
7. Continue switching accounts and approving newly available requests until the checks finish. **Daniel** should be selected at **3.0 ZEC**.
8. As Daniel, review the ZEC invoice, recipient and fee, then approve payment. Wait for confirmation.
9. As Alice, approve NFT delivery. Wait for the Ethereum transaction to confirm.

Expected result: the auction is Sold, Daniel owns the NFT, Alice has received the payment, and both accounts retain the corresponding history. The sale leaves public Market when bidding closes; its participants continue through My portfolio and Messages.

## What the approvals mean

Each protocol approval permits a specific owner action, such as a masked-value conversion or an authorized mask contribution/opening. Independent requests can be ready together. Other requests depend on results that do not exist yet, so one initial approval cannot complete every stage. New messages after earlier approvals are expected.

Protocol approvals are not all blockchain transactions. NFT custody, ZEC payment and NFT delivery have separate transaction reviews. The current platform signs approved actions on the user's behalf using development custody. See [why several approvals are necessary](design-decisions.md#why-private-checks-require-several-approvals) for the design tradeoff.

## Timing and accelerated closing

Bidding normally closes one hour before the end time. Early approvals are optional. Once bidding closes, each participant has one shared ten-minute budget across their ready obligations; overlapping requests count once, and service/dependency waits pause it. Later messages do not replenish the budget.

**Close bidding now** is a seller control available only with the `demo-mode` build feature and localnet runtime. It changes the end time to one hour from the click, using the normal closing path. It does not bypass checks or signatures. Selection proceeds immediately once all required results are available; it does not wait for the hour to elapse.

After full confirmed payment, the seller can approve delivery. If the seller exhausts their remaining response budget, the already accepted publication terms authorize delivery automatically. Partial ZEC payments need a top-up and are not automatically refunded.

## Additional checks

Use a fresh auction for each bidding case; a submitted bid is fixed and cannot be edited.

| Scenario | Expected result |
| --- | --- |
| Bid exactly equals the reserve | The bid qualifies |
| Two equal highest bids | The earlier admitted buyer wins |
| All bids are below the reserve | No sale; Returned appears after the NFT return confirms |
| Open Market while signed out | Open auctions are visible; personal pages lead to sign-in |
| Close bidding, then view as a nonparticipant | The auction is absent from Market and its private details are inaccessible |
| View after closing as seller or admitted buyer | The auction remains accessible through My portfolio |
| Repeat an already accepted approval | It does not create a second action or payment |
| Withdraw an available NFT | Messages shows the exact Ethereum destination; ownership leaves the platform only after confirmation |
| Deposit an external NFT | Send it to the personal address shown by Deposit NFT, then verify its transaction hash, contract and token ID; only a confirmed incoming NFT still held there is accepted |

Use Ethereum localnet (chain 31337) for NFT checks. Do not send public-network assets to these local addresses.

## Automated local-chain checks

With the full stack running, these scripts create assets and transactions. Use them only on a disposable demo:

```bash
python3 scripts/localnet/check.py \
  --url http://localhost:8180 --origin http://localhost:3100 \
  --output /tmp/void-market-sale-check.json

python3 scripts/localnet/check_transfers.py \
  --url http://localhost:8180 --origin http://localhost:3100 \
  --ethereum-url http://localhost:18545 \
  --output /tmp/void-market-nft-check.json
```

The first checks a sale; the second covers NFT withdrawal, redeposit, replay and no-sale return. Keep their output private. A successful source or mocked-browser test does not replace these running-chain checks.

## Start over

To permanently discard this demo's containers, volumes, accounts, keys, chain history and media:

```bash
python3 scripts/localnet/compose.py reset --yes
python3 scripts/localnet/compose.py up
```

This is destructive. Use `stop` to pause without losing data. An arbiter restart interrupts unfinished HE attempts because active numeric masks are not persisted; finish active auctions before restarting when possible. See [Operations and deployment](operations.md) for retained state and recovery details.
