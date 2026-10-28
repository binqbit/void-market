"""Remove one managed Compose localnet without touching other Docker projects."""

import shutil
from pathlib import Path

STATE_DIRECTORIES = ("chains", "media", "artifacts", "identity-parent", "secrets")
STATE_PATHS = {
    "LOCALNET_STATE_DIR": "chains",
    "DEV_MEDIA_ROOT": "media",
    "ARBITER_ARTIFACT_ROOT": "artifacts",
    "CUSTODY_MASTER_KEY_FILE": "secrets/custody.key",
    "ARBITER_SIGNING_KEY_FILE": "secrets/node.key",
    "ARBITER_SERVICE_TOKEN_FILE": "secrets/node.token",
    "DATABASE_PASSWORD_FILE": "secrets/database_password",
    "JWT_SECRET_FILE": "secrets/jwt_secret",
    "ARBITER_DATABASE_PASSWORD_FILE": "secrets/arbiter_database_password",
}


def validate_reset(state, values, project_root):
    if state == project_root or state in project_root.parents or state == Path.home():
        raise SystemExit("Reset requires a dedicated localnet state directory.")

    for name, relative in STATE_PATHS.items():
        expected = state / relative
        configured = Path(values.get(name, expected))
        if configured.resolve() != expected:
            raise SystemExit(f"Cannot reset: {name} must point to {expected}.")

    for name in (*STATE_DIRECTORIES, "compose.env"):
        path = state / name
        if path.is_symlink():
            raise SystemExit(f"Cannot reset a linked state path: {path}")
        if path.exists() and path.is_dir() != (name in STATE_DIRECTORIES):
            raise SystemExit(f"Unexpected localnet state path: {path}")


def reset_localnet(state, values, compose, *, confirmed, project_root):
    validate_reset(state, values, project_root)
    project = values["COMPOSE_PROJECT_NAME"]
    print(f"Reset Compose project: {project}")
    print("Remove its containers, networks, database and Kafka volumes.")
    print(f"Remove chain data, wallets, keys, media and node state under: {state}")

    if not confirmed:
        raise SystemExit("Run reset --yes to confirm permanent deletion.")

    # Keep files and credentials intact if Docker cannot finish the teardown.
    compose("down", "--volumes", "--remove-orphans")

    for name in STATE_DIRECTORIES:
        path = state / name
        if path.exists():
            shutil.rmtree(path)

    # Delete the manifest last so an interrupted cleanup can be retried.
    (state / "compose.env").unlink()
    if not any(state.iterdir()):
        state.rmdir()
    print("Localnet reset. Run up to initialize a fresh demo.")
