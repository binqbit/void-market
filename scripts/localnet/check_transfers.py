#!/usr/bin/env python3
"""Check local NFT withdrawal, redeposit, replay and no-sale custody return."""

import argparse
import json
import time
import uuid
from pathlib import Path

from check import Client, approve_transaction, confirmed, wait_for
from rpc import Rpc


def reject(operation, expected):
    try:
        operation()
    except RuntimeError as error:
        if f": {expected} " not in str(error):
            raise
    else:
        raise AssertionError(f"Expected HTTP {expected}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:8080")
    parser.add_argument("--origin", default="http://localhost:3000")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--ethereum-url", default="http://127.0.0.1:8545")
    args = parser.parse_args()
    alice, bob = [Client(args.url, args.origin, actor) for actor in ("alice", "bob")]
    evidence = {"checks": [], "profile": "localnet"}
    ethereum = Rpc(args.ethereum_url)
    assert int(ethereum.call("eth_chainId"), 16) == 31337
    external, different = ethereum.call("eth_accounts")[6:8]
    mint_id = str(uuid.uuid4())
    first = alice.post("/v1/demo/nft", {"request_id": mint_id})
    assert alice.post("/v1/demo/nft", {"request_id": mint_id})["id"] == first["id"]
    minted = confirmed(alice, mint_id)
    asset = minted["asset_id"]
    evidence["checks"].append(minted)
    route = f"/v1/assets/{asset}/withdraw"
    request_id = str(uuid.uuid4())
    body = {"request_id": request_id, "recipient_address": external}
    alice.post(route, body)
    reject(
        lambda: alice.post(route, body | {"recipient_address": different}),
        409,
    )
    review = wait_for(
        "withdrawal review",
        lambda: (
            item
            if (item := alice.get(f"/v1/transactions/{request_id}"))["can_approve"]
            else None
        ),
    )
    reject(
        lambda: bob.post(
            f"/v1/transactions/{request_id}/approve", {"revision": review["revision"]}
        ),
        404,
    )
    reject(
        lambda: alice.post(
            f"/v1/transactions/{request_id}/approve",
            {"revision": review["revision"] - 1},
        ),
        409,
    )
    alice.post(f"/v1/transactions/{request_id}/cancel", {})
    assert alice.get(f"/v1/assets/{asset}")["pending_transaction_id"] is None
    assert alice.get(f"/v1/transactions/{request_id}")["transaction_id"] is None
    request_id = str(uuid.uuid4())
    alice.post(
        route,
        {"request_id": request_id, "recipient_address": external},
    )
    evidence["checks"].append(approve_transaction(alice, request_id))
    withdrawn = alice.get(f"/v1/assets/{asset}")
    assert withdrawn["owner_id"] is None
    assert all(item["id"] != asset for item in alice.get("/v1/me/assets"))
    token = withdrawn["nft"]["token_id"]
    collection = withdrawn["nft"]["collection"]
    owner_call = "0x6352211e" + f"{int(token):064x}"
    owner = ethereum.call("eth_call", [{"to": collection, "data": owner_call}, "latest"])
    assert "0x" + owner[-40:] == external.lower()
    print(
        "PASS withdrawal cancellation, authorization, replay and external ownership",
        flush=True,
    )

    address = alice.get("/v1/me/nft-deposit")["address"]
    transfer_data = (
        "0x42842e0e"
        + external[2:].zfill(64)
        + address[2:].zfill(64)
        + f"{int(token):064x}"
    )
    transaction = ethereum.call(
        "eth_sendTransaction",
        [{
            "from": external,
            "to": collection,
            "data": transfer_data,
            "gas": "0x80000",
            "gasPrice": ethereum.call("eth_gasPrice"),
        }],
    )
    wait_for(
        "external NFT deposit mined",
        lambda: ethereum.call("eth_getTransactionReceipt", [transaction]),
    )
    deposit = {
        "request_id": str(uuid.uuid4()),
        "transaction_id": transaction,
        "collection": collection,
        "token_id": token,
    }
    returned = alice.post("/v1/nft-deposits", deposit)
    assert returned["id"] == asset and returned["owner_id"] == alice.profile["address"]
    assert alice.post("/v1/nft-deposits", deposit)["id"] == asset
    reject(
        lambda: alice.post(
            "/v1/nft-deposits", deposit | {"request_id": str(uuid.uuid4())}
        ),
        409,
    )
    evidence["checks"].append({
        "kind": "external_deposit",
        "transaction_id": transaction,
        "asset_id": asset,
    })
    print("PASS confirmed deposit restores the same NFT without double credit", flush=True)

    mint_id = str(uuid.uuid4())
    alice.post("/v1/demo/nft", {"request_id": mint_id})
    asset = confirmed(alice, mint_id)["asset_id"]
    auction = alice.post(
        "/v1/auctions",
        {
            "request_id": str(uuid.uuid4()),
            "asset_id": asset,
            "reserve": "1.00000000",
            "closes_at_ms": int(time.time() * 1000) + 3_600_000 + 15_000,
        },
    )
    path = f"/v1/auctions/{auction['id']}"
    alice.post(path + "/publish", {"revision": auction["revision"], "consent": True})
    deposit = wait_for(
        "no-sale custody request",
        lambda: next(
            (
                item
                for item in alice.get("/v1/me/transactions")
                if item["auction_id"] == auction["id"] and item["kind"] == "deposit"
            ),
            None,
        ),
    )
    evidence["checks"].append(approve_transaction(alice, deposit["id"]))
    final = wait_for(
        "no-sale automatically returns NFT",
        lambda: (
            item if (item := alice.get(path))["custody_status"] == "returned" else None
        ),
        90,
    )
    assert final["status"] == "no_sale"
    item = alice.get(f"/v1/assets/{asset}")
    assert (
        item["owner_id"] == alice.profile["address"]
        and item["active_auction_id"] is None
    )
    evidence["checks"].extend(
        item
        for item in alice.get("/v1/me/transactions")
        if item["auction_id"] == auction["id"] and item["kind"] == "return"
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(evidence, indent=2) + "\n")
    print("PASS complete NFT recovery flow", flush=True)


if __name__ == "__main__":
    main()
