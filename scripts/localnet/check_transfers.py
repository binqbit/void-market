#!/usr/bin/env python3
"""Check real local NFT transfers, cancellation, replay and no-sale custody return."""

import argparse
import json
import time
import uuid
from pathlib import Path

from check import Client, approve_transaction, confirmed, wait_for


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
    args = parser.parse_args()
    alice, bob, daniel = [
        Client(args.url, args.origin, actor) for actor in ("alice", "bob", "daniel")
    ]
    evidence = {"checks": [], "profile": "localnet"}
    mint_id = str(uuid.uuid4())
    first = alice.post("/v1/demo/nft", {"request_id": mint_id})
    assert alice.post("/v1/demo/nft", {"request_id": mint_id})["id"] == first["id"]
    minted = confirmed(alice, mint_id)
    asset = minted["asset_id"]
    evidence["checks"].append(minted)
    route = f"/v1/assets/{asset}/native-transfer"
    request_id = str(uuid.uuid4())
    body = {"request_id": request_id, "recipient_address": bob.profile["address"]}
    alice.post(route, body)
    reject(
        lambda: alice.post(
            route, body | {"recipient_address": daniel.profile["address"]}
        ),
        409,
    )
    review = wait_for(
        "transfer review",
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
        {"request_id": request_id, "recipient_address": daniel.profile["address"]},
    )
    evidence["checks"].append(approve_transaction(alice, request_id))
    assert alice.get(f"/v1/assets/{asset}")["owner_id"] == daniel.profile["address"]
    assert (
        len(
            [
                item
                for item in daniel.get("/v1/me/transfers")
                if item["asset_id"] == asset
            ]
        )
        == 1
    )
    print(
        "PASS transfer cancellation, authorization, exact replay and native ownership",
        flush=True,
    )

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
