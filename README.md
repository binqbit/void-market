# Void Market

Void Market is a privacy-focused auction marketplace for digital assets, designed to keep bids and sale prices confidential. It connects sellers with eligible buyers through auctions whose financial terms do not need to become public information.

The protocol is designed to coordinate sellers and buyers through encrypted computations and cryptographic verification, with an automated arbiter managing the auction and settlement workflow. The aim is to select the winning bid and coordinate the exchange of an asset without requiring public disclosure of the amounts involved.

Void Market brings private price discovery and coordinated asset exchange into one marketplace experience. This prototype explores that design; the documentation explains its operation, scope, and current guarantees.

## Run locally

Requires Linux x86-64, Docker with the Compose plugin and Python 3.

Run commands from the repository root. If your user needs `sudo` to access Docker, prefix the commands below with `sudo`.

```bash
python3 scripts/localnet/compose.py up
```

Open the application at **http://localhost:3100**.

## Service commands

| Command | Action |
| --- | --- |
| `python3 scripts/localnet/compose.py status` | Show service status |
| `python3 scripts/localnet/compose.py logs` | Show service logs |
| `python3 scripts/localnet/compose.py stop` | Stop services and preserve data |
| `python3 scripts/localnet/compose.py up` | Start services again |
| `python3 scripts/localnet/compose.py down` | Remove containers and networks; preserve data |
| `python3 scripts/localnet/compose.py reset --yes` | Remove the stack and all local data, including volumes, keys and chain history |

## Documentation

- [Concept](docs/concept.md)
- [How it works](docs/how-it-works.md)
- [Plain-language explanation](docs/protocol-explained.md)
- [Cryptographic protocol](docs/cryptographic-protocol.md)
- [Protocol by Example](docs/protocol-by-example.md)
- [Design decisions](docs/design-decisions.md)
- [Operations and deployment](docs/operations.md)
- [Service architecture](docs/runtime-architecture.md)
- [Demo guide](docs/demo.md)
