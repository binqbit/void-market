# Operations and deployment

The commands below manage the full local-chain demo, including its arbiter. Run them from the repository root. Keep development state separate from any deployment that must be retained.

## Configuration

`python3 scripts/localnet/compose.py up` initializes `.localnet/compose.env` and private files once. Later commands reuse that manifest, including the Compose project name and container UID/GID. They do not regenerate working wallet or custody keys. A different `--state` directory is independent only when its project name and published ports are also distinct.

| Setting | Default in the launcher | Purpose |
| --- | --- | --- |
| `COMPOSE_PROJECT_NAME` | `void-market-localnet` | Container, network and volume identity; retained after initialization |
| `WEB_PORT` | `3100` | Host-loopback web port |
| `HTTP_PORT` | `8180` | Host-loopback API port |
| `ORIGIN` | `http://localhost:3100` | Canonical browser origin and OAuth callback base |
| `RP_ID` | `localhost` | Account credential relying-party domain |
| `ETHEREUM_RPC_PORT` | `18545` | Host-loopback Anvil RPC |
| `ZCASH_RPC_PORT` | `18332` | Host-loopback Zebra RPC |
| `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` | Empty | Optional Google web login |

For durable local changes, edit `.localnet/compose.env` without deleting its generated entries. Exported environment values can override Compose settings such as origin and ports for a command; use the same overrides consistently. The launcher pins the saved project and UID/GID so a later shell or `sudo` invocation cannot silently select a different stack.

The root `.env.example` and `docker/*.env.example` describe direct/base Compose or native-service configuration. The full-demo launcher explicitly uses its own `compose.env`; it does not import the repository's `.env` automatically. Do not paste credentials into README, logs or version control.

The `demo-mode` Cargo feature supplies the demo-only auction close capability. The full-demo images build with it and enable the localnet runtime. `DEMO_MODE=true` selects the local chains; `DEV_CUSTODY=true` enables platform-held user keys; `LOCAL_DEMO=true` permits the arbiter's honest-role mechanics and loopback gateway. These are distinct settings, not a public-network security profile.

## Google login

Create a **Web application** OAuth client in Google Auth Platform. Configure the exact callback URI and browser origin for the selected deployment; Google validates the redirect URI against the client configuration. See [Google's web-server OAuth guide](https://developers.google.com/identity/protocols/oauth2/web-server).

| Environment | Authorized redirect URI | JavaScript origin |
| --- | --- | --- |
| Default local demo | `http://localhost:3100/api/v1/auth/google/callback` | `http://localhost:3100` |
| Supplied HTTPS deployment | `https://voidmarket.dev/api/v1/auth/google/callback` | `https://voidmarket.dev` |

Set both Google variables in the private manifest, then run `up`. Leave both empty to disable Google login while retaining Alice/Bob/Daniel demo login. Configure the consent screen and allowed test users in Google for the audience you intend to admit. Email/password registration is not part of the web login flow.

## Routine service management

```bash
python3 scripts/localnet/compose.py up
python3 scripts/localnet/compose.py status
python3 scripts/localnet/compose.py logs
python3 scripts/localnet/compose.py stop
python3 scripts/localnet/compose.py down
```

- `up`: build images, start dependencies, migrate, provision, initialize and wait for service readiness.
- `status`: show this stack's containers.
- `logs`: print service logs; this command does not follow continuously.
- `stop`: stop the containers, preserving them and all retained data.
- `down`: remove containers and networks, preserving local state and named volumes.
- `reset --yes`: remove those containers, networks, volumes and the generated local state. This permanently destroys the local demo's accounts, assets, chain history, keys and media.

Use the same `--state` path on every command for a nondefault deployment. If a command needs Docker privileges, invoke the launcher with `sudo` consistently. Never use a host-wide `docker system prune --volumes` as a project reset.

For individual service operations, define a convenience function using the same manifest and all four files:

```bash
vm_state="$PWD/.localnet"
vm_compose() {
  docker compose --env-file "$vm_state/compose.env" \
    -f compose.yaml -f compose.arbiter.yaml \
    -f compose.custody.yaml -f compose.localnet.yaml "$@"
}

vm_compose logs --tail=100 -f api worker arbiter
vm_compose ps -a
vm_compose config --quiet
```

Run this shell with the intended deployment environment and without a conflicting `COMPOSE_PROJECT_NAME`. Prefer the Python launcher for start/stop/reset because it pins the saved stack identity. Avoid printing `docker compose config` without `--quiet`: expanded settings may include Google credentials.

## Readiness and common failures

The API exposes `/health/live` and `/health/ready`; its readiness checks database migrations. The worker's readiness also reflects event delivery and enabled local-chain progress. An API returning 200 does not by itself prove that the arbiter or chains are ready. Check all services with `status`.

```bash
curl --fail http://localhost:8180/health/ready
curl --fail http://localhost:3100/api/health/ready
```

The one-shot `migrate`, `arbiter-migrate` and `kafka-init` containers should exit successfully. A failed one-shot service needs investigation before restarting dependent services.

| Symptom | First check |
| --- | --- |
| Docker permission denied | Use an account with Docker access, or `sudo` for the launcher |
| Private file permission error after a sudo launch | Rerun the launcher; it restores generated file ownership to the retained container identity |
| Port already allocated | Inspect the other listener; choose unused ports without deleting retained state |
| Google redirect rejected | Compare `ORIGIN`, the client callback URI and the browser URL exactly |
| ZEC claim waiting | Inspect `localnet-miner`, `localnet-faucet`, `zallet` and `worker`; funding, wallet sync and confirmation are distinct steps |
| No new approvals | Check Messages, bidding-close time, worker/arbiter health and the auction result; a terminal auction does not create more signing requests |
| Transaction status `reconcile` | Inspect the stored transaction and chain evidence; do not create another spend or manually mark it confirmed |
| Lost custody key or node identity | Restore the matching backup; replacing keys does not restore encrypted records or an accepted custody assignment |

## Updates, backups and restore

Before updating a retained demo, finish active auctions where possible. An arbiter restart interrupts unfinished HE attempts because numeric masks are not persisted. Waiting chain transactions are reconciled using their retained signed bytes.

1. Stop the stack with the launcher using its normal manifest and environment.
2. Back up the entire private state directory, retaining file owners and permissions. This includes the manifest, secrets, both node-identity copies, chain databases, wallet state, media and check artifacts.
3. Back up **all named volumes for this Compose project** while stopped. These include the market database, arbiter database and Kafka data. List the exact names with `docker volume ls --filter label=com.docker.compose.project=void-market-localnet`; substitute the saved project name for a server deployment.
4. Record the source revision and built image IDs alongside the backup. Keep the backup private: it contains wallet and custody keys.
5. Update the checkout and run `up`. It rebuilds images, applies pending migrations and reuses the existing identity and state.

For restore, use an empty, stopped target stack. Restore the matching state directory and named volumes together, with their original ownership and paths, then start the matching application revision and verify readiness. Test the restored accounts, balances and asset ownership before allowing new work. A database-only backup cannot reconstruct missing custody keys or local chain history. Restoring an old application over newer database migrations is not a supported rollback.

## HTTPS server deployment

The checked-in `deploy/voidmarket.dev/` directory is a single-host demo configuration. It contains public routing settings, not SSH credentials or certificates. Connect using your own operator account and key, for example `ssh -i ~/.ssh/void-market operator@SERVER_IP`; do not copy private SSH material into the repository.

Install Docker Engine and its Compose plugin using the host OS instructions, plus Python 3 and Nginx. For Ubuntu, use [Docker's official installation guide](https://docs.docker.com/engine/install/ubuntu/). Keep SSH access available while configuring the host firewall. The application/RPC ports in Compose bind to loopback; only SSH and HTTP/HTTPS need external access for this topology.

Check out the application, for example at `/opt/void-market`, and prepare a dedicated private state directory such as `/var/lib/void-market`. Run initialization as the intended operating account or from an administrative shell. Before each launcher command, load the public deployment settings:

```bash
cd /opt/void-market
set -a
. deploy/voidmarket.dev/runtime.env
set +a
python3 scripts/localnet/compose.py up --state /var/lib/void-market
```

`runtime.env` selects the domain, loopback ports and initial Compose project name. If using another domain, edit its `ORIGIN`/`RP_ID`, every domain and certificate path in the Nginx file, and the Google client configuration together. DNS must point to the server.

Install a trusted certificate and its private key at the paths expected by the Nginx configuration. On a Debian/Ubuntu-style Nginx installation:

```bash
sudo install -d -m 0700 /etc/nginx/tls/voidmarket.dev
# Install fullchain.pem and privkey.pem here from your certificate provider.
sudo chmod 0600 /etc/nginx/tls/voidmarket.dev/privkey.pem
sudo install -m 0644 deploy/voidmarket.dev/nginx.conf \
  /etc/nginx/sites-available/voidmarket.dev
sudo ln -sfn /etc/nginx/sites-available/voidmarket.dev \
  /etc/nginx/sites-enabled/voidmarket.dev
sudo nginx -t
sudo systemctl reload nginx
```

The site file belongs inside the Nginx `http` context. The certificate commands do not issue or renew a certificate: configure renewal with your certificate provider and reload Nginx after renewal. The supplied ACME challenge route is available for a webroot-based renewal setup.

| Public path | Destination |
| --- | --- |
| HTTP `/` | Redirect to HTTPS |
| `/api/v1/messages/events` | API SSE stream, buffering disabled |
| `/api/node/` | Blocked at the public host proxy |
| Other `/api/` paths | `127.0.0.1:8180`, with `/api/` removed |
| Other paths | `127.0.0.1:3100`, including SPA deep links |

Do not enable response buffering or caching for the Messages stream. Nginx supports both explicit `proxy_buffering off` and the upstream `X-Accel-Buffering` header; see its [proxy buffering reference](https://nginx.org/en/docs/http/ngx_http_proxy_module.html#proxy_buffering). Access logs omit query strings so OAuth callback codes are not written to them.

After deployment, verify HTTPS home and deep links, `/api/health/ready`, Google callback configuration, demo login and an entire disposable sale. Check status and logs after a stop/start as well. A domain with HTTPS still runs the same custodial local-network demo; it does not make its assets real-network assets.

## Verification

Use the Rust toolchain pinned in `rust-toolchain.toml`. Native builds need a C/C++ toolchain, `pkg-config` and OpenSSL development headers. The frontend image pins Node 24; its package manifest specifies the supported minimum.

Native HE tests use U8 by default and at most U16. Service tests use small zatoshi amounts and a U8 auction profile in their isolated databases. Full-width arithmetic boundaries, width policies and ZEC formatting are tested without expensive HE operations. These fixtures do not change the application's normal 64-bit auction profile.

```bash
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features -- --test-threads=1
python3 -m unittest discover -s tests/localnet -v
```

```bash
cd apps/web
npm ci --ignore-scripts
npm run check
npm test
npm run build
npx playwright install chromium
npm run test:browser
```

Database tests require `TEST_DATABASE_URL` pointing to a disposable PostgreSQL instance. Kafka tests also require `TEST_KAFKA_BROKERS` and the topics `test.voidmarket.assets.events.v1` and `test.voidmarket.nodes.v1`, each with one partition. Never run destructive or synthetic integration fixtures against retained user data.

```bash
cargo test --locked -p market --all-features --test api -- --ignored --test-threads=1
```

For each arbiter integration binary (`control_plane`, `custodial_messages`, `async_approvals`, `auction_sales`), create fresh one-partition topics and export their names as `TEST_EVENT_TOPIC` and `TEST_NODE_TOPIC`, then run:

```bash
cargo test --locked -p arbiter --all-features --test auction_sales \
  -- --ignored --test-threads=1
```

`private_comparison` is an additional ignored native HE test and does not need Kafka. Set `TEST_ARTIFACT_ROOT` and `TEST_MEDIA_ROOT` to disposable directories if you want the generated test files outside the system temporary directory.

Browser tests use mocked APIs unless the dedicated real-API case is supplied `WEB_API_FIXTURE_FILE`. That file contains isolated `owner` and `recipient` objects, each with `address` and an active session `token`; it is private test material. Point Vite at the matching API using `API_TARGET`. The actual API test performs an upload, ownership transfer and logout; it must not use production accounts. `PLAYWRIGHT_CHROMIUM_EXECUTABLE` can select an installed Chromium instead of a Playwright download.

The maintained `scripts/localnet/check.py` and `check_transfers.py` exercise running local chains and create/modify demo state. Run them only against an explicitly disposable demo, with `--url`, `--origin` and `--output` set for that stack. They are not read-only health checks.
