#!/usr/bin/env python3
"""Exercise a running localnet through public APIs; creates new demo assets only."""

import argparse
import http.cookiejar
import json
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path


class Client:
    def __init__(self, url, origin, alias):
        self.url = url.rstrip("/")
        self.origin = origin
        self.opener = urllib.request.build_opener(
            urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar())
        )
        self.call("POST", "/v1/demo/login", {"actor": alias})
        self.profile = self.call("GET", "/v1/me")

    def call(self, method, path, body=None):
        request = urllib.request.Request(
            self.url + path,
            data=json.dumps(body).encode() if body is not None else None,
            method=method,
            headers={"Origin": self.origin, "Content-Type": "application/json"},
        )
        try:
            with self.opener.open(request, timeout=30) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            raise RuntimeError(
                f"{method} {path}: {error.code} {error.read().decode()}"
            ) from error

    def get(self, path):
        return self.call("GET", path)

    def post(self, path, body):
        return self.call("POST", path, body)


def wait_for(description, check, timeout=180):
    started = time.monotonic()
    while time.monotonic() - started < timeout:
        result = check()
        if result:
            print(f"PASS {description} ({time.monotonic() - started:.1f}s)", flush=True)
            return result
        time.sleep(1)
    raise TimeoutError(description)


def confirmed(client, operation):
    def inspect():
        result = client.get(f"/v1/transactions/{operation}")
        if result["status"] in ("failed", "cancelled", "reconcile"):
            raise RuntimeError(
                f"Transaction did not confirm: {result['status']}: {result['error']}"
            )
        return result if result["status"] == "confirmed" else None

    return wait_for("native transaction confirmed", inspect, 300)


def approve_transaction(client, operation):
    review = wait_for(
        "transaction ready for review",
        lambda: (
            item
            if (item := client.get(f"/v1/transactions/{operation}"))["can_approve"]
            else None
        ),
    )
    body = {"revision": review["revision"]}
    client.post(f"/v1/transactions/{operation}/approve", body)
    client.post(f"/v1/transactions/{operation}/approve", body)
    return confirmed(client, operation)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:8080")
    parser.add_argument("--origin", default="http://localhost:3000")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--seller-timeout",
        action="store_true",
        help="Wait the real remaining seller budget instead of clicking final approval",
    )
    parser.add_argument(
        "--resume", help="Resume a retained auction after restarting the application"
    )
    args = parser.parse_args()
    started = time.time()
    actors = {
        name: Client(args.url, args.origin, name) for name in ("alice", "bob", "daniel")
    }
    alice, bob, daniel = (actors[name] for name in ("alice", "bob", "daniel"))
    evidence = {"profile": "localnet", "started_at": started, "checks": []}

    if args.resume:
        auction = alice.get(f"/v1/auctions/{args.resume}")
        result = auction
        asset_id = auction["asset"]["id"]
        path = f"/v1/auctions/{args.resume}"
        approved = {
            item["id"]
            for actor in actors.values()
            for item in actor.get("/v1/messages")["messages"]
            if item["auction_id"] == args.resume and item["status"] == "completed"
        }
        assert len(approved) == 21
        assert result["winner"]["address"] == bob.profile["address"]
        assert result["agreed_price"] == "1.50000000"
        evidence["resumed_after_application_restart"] = True
        evidence["checks"] = [
            item
            for item in bob.get("/v1/me/transactions")
            if item["auction_id"] == args.resume and item["status"] == "confirmed"
        ]
    else:
        for buyer in (bob, daniel):
            balance = buyer.get("/v1/demo")["own"]["balance"]
            if balance is None or float(balance) < 2:
                operation = buyer.post(
                    "/v1/demo/zec", {"request_id": str(uuid.uuid4())}
                )
                evidence["checks"].append(confirmed(buyer, operation["id"]))
        mint = alice.post("/v1/demo/nft", {"request_id": str(uuid.uuid4())})
        minted = confirmed(alice, mint["id"])
        asset_id = minted["asset_id"]
        evidence["checks"].append(minted)

        auction = alice.post(
            "/v1/auctions",
            {
                "request_id": str(uuid.uuid4()),
                "asset_id": asset_id,
                "reserve": "1.00000000",
                "closes_at_ms": int(time.time() * 1000) + 3_600_000 + 45_000,
            },
        )
        path = f"/v1/auctions/{auction['id']}"
        auction = alice.post(
            path + "/publish", {"revision": auction["revision"], "consent": True}
        )
        deposit = wait_for(
            "custody review prepared",
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
        auction = alice.get(path)
        assert auction["status"] == "open", auction["status"]
        for buyer, amount in ((bob, "1.50000000"), (daniel, "1.25000000")):
            buyer.post(
                path + "/bids",
                {
                    "request_id": str(uuid.uuid4()),
                    "amount": amount,
                    "policy_hash": auction["policy_hash"],
                },
            )

        approved = set()

        def matching():
            state = alice.get(path)
            if state["status"] in ("interrupted", "no_sale"):
                raise RuntimeError(
                    f"Auction failed: {state['status']} {state['reason']}"
                )
            if state["status"] == "awaiting_completion":
                return state
            for actor in actors.values():
                for item in actor.get("/v1/messages")["messages"]:
                    if (
                        item["auction_id"] != auction["id"]
                        or item["status"] != "ready"
                        or item["id"] in approved
                    ):
                        continue
                    actor.post(
                        f"/v1/messages/{item['id']}/approve",
                        {
                            "request_id": item["request_id"],
                            "revision": item["revision"],
                        },
                    )
                    approved.add(item["id"])
            return None

        result = wait_for(
            "real private comparisons and winner selection", matching, 1500
        )
        assert result["winner"]["address"] == bob.profile["address"]
        assert result["agreed_price"] == "1.50000000"
        assert daniel.get(path)["agreed_price"] is None
        assert len(approved) == 21, len(approved)
    evidence["auction"] = auction["id"]
    evidence["private_approvals"] = len(approved)

    if bob.get(path)["payment"]["remaining"] == "1.50000000":
        payment = bob.post(
            path + "/payments",
            {
                "request_id": str(uuid.uuid4()),
                "generation": result["generation"],
                "amount": "0.50000000",
            },
        )
        evidence["checks"].append(approve_transaction(bob, payment["id"]))
    partial = bob.get(path)
    assert partial["payment"]["remaining"] == "1.00000000", partial["payment"]
    assert partial["custody_status"] == "held"
    topup = wait_for(
        "top-up invoice prepared",
        lambda: next(
            (
                item
                for item in bob.get("/v1/me/transactions")
                if item["auction_id"] == auction["id"]
                and item["kind"] == "payment"
                and item["status"] == "ready"
            ),
            None,
        ),
    )
    evidence["checks"].append(approve_transaction(bob, topup["id"]))
    paid = alice.get(path)
    assert paid["payment_status"] == "confirmed", paid["payment_status"]
    evidence["seller_budget_before_delivery_ms"] = paid["participation"]["remaining_ms"]
    evidence["payment_confirmed_at"] = time.time()
    if not args.seller_timeout:
        alice.post(
            path + "/complete",
            {
                "request_id": paid["completion_request_id"],
                "generation": paid["generation"],
            },
        )
    final = wait_for(
        "automatic NFT delivery after seller timeout"
        if args.seller_timeout
        else "NFT delivery after seller approval",
        lambda: state if (state := alice.get(path))["status"] == "completed" else None,
        750,
    )
    asset = bob.get(f"/v1/assets/{asset_id}")
    assert asset["owner_id"] == bob.profile["address"], asset
    delivery = next(
        item
        for item in bob.get("/v1/me/transactions")
        if item["auction_id"] == auction["id"] and item["kind"] == "delivery"
    )
    evidence["checks"].append(delivery)
    evidence["seller_timeout"] = args.seller_timeout
    evidence["settlement_note"] = final["settlement_note"]
    evidence["finished_at"] = time.time()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(evidence, indent=2) + "\n")
    print(f"PASS end-to-end local sale; evidence: {args.output}", flush=True)


if __name__ == "__main__":
    main()
