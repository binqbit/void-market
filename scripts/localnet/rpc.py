"""Small JSON-RPC client for local development node administration."""

import base64
import json
import urllib.request
from pathlib import Path


class RpcError(RuntimeError):
    def __init__(self, method, code):
        super().__init__(f"{method} rejected by local node (code {code})")
        self.code = code


class Rpc:
    def __init__(self, url, cookie=None):
        self.url = url
        self.cookie = Path(cookie) if cookie else None

    def call(self, method, params=None, timeout=180):
        headers = {"Content-Type": "application/json"}
        if self.cookie:
            token = base64.b64encode(self.cookie.read_bytes().strip()).decode()
            headers["Authorization"] = "Basic " + token
        request = urllib.request.Request(
            self.url,
            headers=headers,
            data=json.dumps(
                {
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": method,
                    "params": params or [],
                }
            ).encode(),
        )
        with urllib.request.urlopen(request, timeout=timeout) as response:
            result = json.load(response)
        if result.get("error"):
            raise RpcError(method, result["error"]["code"])
        return result["result"]
