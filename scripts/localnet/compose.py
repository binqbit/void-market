#!/usr/bin/env python3
"""Start an isolated Docker localnet. Example: python3 scripts/localnet/compose.py up

up, stop, status, logs and down preserve chain data, keys and Docker volumes.
reset --yes removes this stack's containers, volumes and generated local state.
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

from cleanup import reset_localnet
from rpc import Rpc

ROOT = Path(__file__).resolve().parents[2]


def set_private_permissions(path, values):
    uid, gid = int(values["LOCAL_UID"]), int(values["LOCAL_GID"])
    metadata = path.stat()
    path.chmod(0o700 if path.is_dir() else 0o600)

    # sudo may create files as root while containers retain the original UID.
    if (metadata.st_uid, metadata.st_gid) != (uid, gid):
        os.chown(path, uid, gid)


def save(path, values):
    path.write_text("".join(f"{key}={value}\n" for key, value in values.items()))
    set_private_permissions(path, values)


def prepare_secret_files(state, values):
    directory = state / "secrets"
    directory.mkdir(mode=0o700, exist_ok=True)
    set_private_permissions(directory, values)

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

        # PostgreSQL reads its secret as root before switching to its database user.
        set_private_permissions(path, values)
        values[file_variable] = str(path)
        values.pop(name, None)


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "command", choices=["up", "stop", "status", "logs", "down", "reset"]
    )
    parser.add_argument("--state", type=Path, default=ROOT / ".localnet")
    parser.add_argument(
        "--yes", action="store_true", help="Confirm permanent deletion with reset"
    )
    args = parser.parse_args()
    if args.yes and args.command != "reset":
        parser.error("--yes is only supported with reset")

    state = args.state.resolve()
    if args.command == "up":
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
            "COMPOSE_PROJECT_NAME": os.environ.get(
                "COMPOSE_PROJECT_NAME", "void-market-localnet"
            ),
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

    project = values.get("COMPOSE_PROJECT_NAME")
    if not project:
        raise SystemExit("The retained Compose project name is missing.")

    compose_environment = os.environ.copy()
    compose_environment.update(
        COMPOSE_PROJECT_NAME=project,
        LOCAL_UID=values["LOCAL_UID"],
        LOCAL_GID=values["LOCAL_GID"],
    )

    command = ["docker", "compose", "--env-file", str(env_file)]
    if args.command in ("down", "reset"):
        command += ["--project-name", project]

        # Cleanup always targets the saved stack, regardless of shell overrides.
        compose_environment.update(values)
        for name in ("DATABASE_PASSWORD", "JWT_SECRET", "ARBITER_DATABASE_PASSWORD"):
            file_variable = name + "_FILE"
            compose_environment[file_variable] = values.get(
                file_variable, str(state / "secrets" / name.lower())
            )

    for filename in (
        "compose.yaml",
        "compose.arbiter.yaml",
        "compose.custody.yaml",
        "compose.localnet.yaml",
    ):
        command += ["-f", str(ROOT / filename)]

    def compose(*arguments):
        subprocess.run(
            command + list(arguments), cwd=ROOT, env=compose_environment, check=True
        )

    if args.command == "reset":
        reset_localnet(state, values, compose, confirmed=args.yes, project_root=ROOT)
        return
    if args.command == "down":
        compose("down", "--remove-orphans")
        return

    prepare_secret_files(state, values)
    for name in (
        "CUSTODY_MASTER_KEY_FILE",
        "ARBITER_SIGNING_KEY_FILE",
        "ARBITER_SERVICE_TOKEN_FILE",
    ):
        set_private_permissions(Path(values[name]), values)
    save(env_file, values)

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
    set_private_permissions(identity_parent, values)
    if identity.exists():
        set_private_permissions(identity, values)
        for name in ("node.json", "signing.key", "service.token"):
            set_private_permissions(identity / name, values)

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
                f"{values['LOCAL_UID']}:{values['LOCAL_GID']}",
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
    set_private_permissions(local_provision, values)
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
