import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ACCOUNT = "00000000-0000-4000-8000-000000000001"


class ChainFixture:
    def __init__(self):
        self.height = 1000
        self.pending = True
        self.generated = []
        self.spendable = 0
        self.incoming = 0
        self.operation_status = "failed"
        self.operation_known = True
        self.confirmations = 0
        self.shield_requests = 0
        self.lock = threading.Lock()
        fixture = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass

            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                with fixture.lock:
                    result = fixture.call(body["method"], body["params"])
                encoded = json.dumps(
                    {"jsonrpc": "2.0", "id": 1, "result": result}
                ).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(encoded)))
                self.end_headers()
                self.wfile.write(encoded)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.server.server_port}"
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def call(self, method, params=None, timeout=None):
        params = params or []
        tip = {"height": self.height, "blockhash": str(self.height)}
        if method == "getblockchaininfo":
            return {
                "chain": "test",
                "blocks": self.height,
                "upgrades": {"37a5165b": {"activationheight": 2, "status": "active"}},
            }
        if method == "getblockcount":
            return self.height
        if method == "getrawmempool":
            return ["pending-payment"] if self.pending else []
        if method in ("generate", "generatetoaddress"):
            self.generated.append(params[0])
            self.height += params[0]
            return [str(self.height)]
        if method == "getwalletstatus":
            return {"node_tip": tip, "wallet_tip": tip, "locked": False}
        if method == "z_listaccounts":
            return [
                {
                    "account_uuid": ACCOUNT,
                    "name": "Void Market local faucet",
                    "addresses": [{"ua": "unified"}],
                }
            ]
        if method == "z_getaddressforaccount":
            return {"address": "uregtest1faucet"}
        if method == "z_listunifiedreceivers":
            return {"p2pkh": "tmFaucet"}
        if method == "z_getbalanceforaccount":
            return {"pools": {"ironwood": {"valueZat": self.spendable}}}
        if method == "z_getbalances":
            return {
                "accounts": [
                    {
                        "account_uuid": ACCOUNT,
                        "ironwood": {
                            "spendable": {"valueZat": self.spendable},
                            "pending": {"valueZat": self.incoming},
                        },
                        "transparent": {
                            "coinbase": {"spendable": {"valueZat": 100000000000}}
                        },
                    }
                ]
            }
        if method == "z_shieldcoinbase":
            self.shield_requests += 1
            return {"opid": "refill"}
        if method == "z_getoperationstatus":
            if not self.operation_known:
                return []
            return [
                {
                    "id": "refill",
                    "status": self.operation_status,
                    "error": {"code": -4, "message": "expired"},
                    "result": {"txid": "funding-transaction"},
                }
            ]
        if method == "getrawtransaction":
            return {"confirmations": self.confirmations}
        if method == "z_listoperationids":
            return ["refill"]
        raise AssertionError(f"Unexpected RPC method: {method}")

    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()
