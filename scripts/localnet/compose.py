#!/usr/bin/env python3
"""Start an isolated Docker localnet. Example: python3 scripts/localnet/compose.py up

Commands preserve all retained chain and wallet data. No command deletes volumes.
The default web address is http://localhost:3100; the API uses localhost:8180.
"""

import argparse
import base64
import json
import os
import secrets
import subprocess
import uuid
from pathlib import Path

from rpc import Rpc

ROOT = Path(__file__).resolve().parents[2]


def save(path, values):
    path.write_text("".join(f"{key}={value}\n" for key, value in values.items()))
    path.chmod(0o600)


def prepare_secret_files(state, values):
    directory = state / "secrets"
    directory.mkdir(mode=0o700, exist_ok=True)
    directory.chmod(0o700)

    for name in ("DATABASE_PASSWORD", "JWT_SECRET", "ARBITER_DATABASE_PASSWORD"):
        file_variable = name + "_FILE"
        path = Path(values.get(file_variable, directory / name.lower()))
        previous = values.get(name)

        if path.exists():
            if previous is not None and path.read_text() != previous:
                raise RuntimeError(f"Retained {file_variable} does not match its previous value")
        elif previous is not None:
            path.write_text(previous)
        else:
            raise RuntimeError(f"Restore the retained {file_variable} before restarting")

        # Application containers use the host UID. PostgreSQL reads its secret
        # as root before switching to its database user.
        path.chmod(0o600)
        values[file_variable] = str(path)
        values.pop(name, None)


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("command", choices=["up", "stop", "status", "logs"])
    parser.add_argument("--state", type=Path, default=ROOT / ".localnet")
    args = parser.parse_args()
    state = args.state.resolve()
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.umask(0o077)
    env_file = state / "compose.env"
    if env_file.exists():
        values = dict(
            line.split("=", 1) for line in env_file.read_text().splitlines() if line
        )
    elif args.command == "up":
        for directory in ("chains", "media", "artifacts", "secrets", "identity-parent"):
            (state / directory).mkdir(mode=0o700)
        master = state / "secrets" / "custody.key"
        master.write_bytes(secrets.token_bytes(32))
        values = {
            "COMPOSE_PROJECT_NAME": "void-market-localnet",
            "LOCAL_UID": str(os.getuid()),
            "LOCAL_GID": str(os.getgid()),
            "LOCALNET_STATE_DIR": str(state / "chains"),
            "DEV_MEDIA_ROOT": str(state / "media"),
            "ARBITER_ARTIFACT_ROOT": str(state / "artifacts"),
            "CUSTODY_MASTER_KEY_FILE": str(master),
            "ARBITER_SIGNING_KEY_FILE": str(state / "secrets" / "node.key"),
            "ARBITER_SERVICE_TOKEN_FILE": str(state / "secrets" / "node.token"),
            "DATABASE_PASSWORD": secrets.token_hex(24),
            "JWT_SECRET": secrets.token_hex(32),
            "ARBITER_DATABASE_PASSWORD": secrets.token_hex(24),
            "KAFKA_CLUSTER_ID": base64.urlsafe_b64encode(uuid.uuid4().bytes)
            .decode()
            .rstrip("="),
            "NODE_ID": str(uuid.uuid4()),
            "DEPLOYMENT_ID": str(uuid.uuid4()),
            "HTTP_PORT": "8180",
            "WEB_PORT": "3100",
            "ORIGIN": "http://localhost:3100",
            "ETHEREUM_RPC_PORT": "18545",
            "ZCASH_RPC_PORT": "18332",
            "GOOGLE_CLIENT_ID": "",
            "GOOGLE_CLIENT_SECRET": "",
        }
        # Compose validates secret source paths before the identity is generated.
        for name in ("ARBITER_SIGNING_KEY_FILE", "ARBITER_SERVICE_TOKEN_FILE"):
            Path(values[name]).touch(mode=0o600)
        save(env_file, values)
    else:
        raise SystemExit("This localnet has not been initialized. Run up first.")

    prepare_secret_files(state, values)
    save(env_file, values)

    command = ["docker", "compose", "--env-file", str(env_file)]
    for filename in (
        "compose.yaml",
        "compose.arbiter.yaml",
        "compose.custody.yaml",
        "compose.localnet.yaml",
    ):
        command += ["-f", str(ROOT / filename)]

    def compose(*arguments):
        subprocess.run(command + list(arguments), cwd=ROOT, check=True)

    if args.command != "up":
        compose({"stop": "stop", "status": "ps", "logs": "logs"}[args.command])
        return
    compose("build", "api", "arbiter", "anvil", "web", "kafka")
    compose(
        "up",
        "-d",
        "--wait",
        "db",
        "kafka",
        "anvil",
        "zebra",
        "zallet",
        "localnet-miner",
        "localnet-faucet",
    )
    rpc = Rpc(f"http://127.0.0.1:{values['ETHEREUM_RPC_PORT']}")
    identity_parent = state / "identity-parent"
    identity = identity_parent / "node"
    if not (identity / "node.json").exists():
        address = rpc.call("eth_accounts")[4]
        subprocess.run(
            [
                "docker",
                "run",
                "--rm",
                "--network",
                "none",
                "--user",
                f"{os.getuid()}:{os.getgid()}",
                "-e",
                f"DEPLOYMENT_ID={values['DEPLOYMENT_ID']}",
                "-e",
                f"ETHEREUM_CUSTODY_ADDRESS={address}",
                "-v",
                f"{identity_parent}:/identity",
                "void-market-arbiter-demo:local",
                "identity",
                "/identity/node",
            ],
            check=True,
        )
        for source, destination in (
            ("signing.key", "ARBITER_SIGNING_KEY_FILE"),
            ("service.token", "ARBITER_SERVICE_TOKEN_FILE"),
        ):
            Path(values[destination]).write_bytes((identity / source).read_bytes())
        node = json.loads((identity / "node.json").read_text())
        values["NODE_ID"] = node["node_id"]
        save(env_file, values)
    provision = json.loads((identity / "node.json").read_text())
    provision["token_file"] = "/run/secrets/arbiter_service_token"
    local_provision = state / "secrets" / "node.json"
    local_provision.write_text(json.dumps(provision))
    compose("run", "--rm", "--no-deps", "kafka-init")
    compose("run", "--rm", "--no-deps", "migrate")
    compose(
        "run",
        "--rm",
        "--no-deps",
        "-v",
        f"{local_provision}:/setup/node.json:ro",
        "-v",
        f"{values['ARBITER_SERVICE_TOKEN_FILE']}:/run/secrets/arbiter_service_token:ro",
        "migrate",
        "provision-node",
        "/setup/node.json",
    )
    compose("run", "--rm", "--no-deps", "migrate", "init-demo")
    compose("up", "-d", "--wait", "api", "worker", "arbiter", "web")
    print(f"Localnet ready: {values['ORIGIN']}/demo. State is retained at {state}.")


if __name__ == "__main__":
    main()
