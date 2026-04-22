# bastion-app

Block-aware unified terminal client. One device-owned sidebar that
lists every "thing you can open a shell/thread to": DD enclaves, SSH
hosts, Anthropic API keys, GitHub tokens, local shells. Built
explicitly as **CLI + desktop** so the architecture can settle before
we touch browser quirks.

Not to be confused with the archived [`devopsdefender/bastion`][v1]
(v1, web-only, lives at [`devopsdefender/dd:crates/bastion`][dd] now).

[v1]: https://github.com/devopsdefender/bastion
[dd]: https://github.com/devopsdefender/dd/tree/main/crates/bastion

## Status

Works today:

- **CLI** (`cargo build` at repo root):
  - `bastion whoami` — device identity fingerprint + X25519 pubkey
  - `bastion list` / `add dd-enclave|anthropic` / `rm`
  - `bastion connect <id>` — interactive shell on a DD enclave
    via native Noise_IK handshake over `/noise/shell/{id}`
- **Desktop** (`cd crates/desktop/src-tauri && cargo tauri dev`):
  - Sidebar lists connectors, shows device fingerprint
  - Add/remove DD-enclave connectors
  - Session pane placeholder (launch lands next milestone)

Next milestones:

- Session launch inside the desktop UI (reuse the same
  `bastion_core::dd_enclave` path the CLI already uses)
- OSC 133 block parser wired through both frontends
- TDX quote verification before pinning the Noise pubkey
- Signal-style multi-device sync (identity seed as the pairing root)

## Layout

```
crates/
├── core/                 — shared types (identity, connector,
│                           store, noise, dd_enclave)
├── cli/                  — `bastion` binary
└── desktop/              — Tauri v2 app (OUT of workspace so
    ├── package.json      —   cargo build at repo root doesn't
    ├── src/              —   need GTK/WebKit headers)
    └── src-tauri/
```

## Building the desktop app

Deliberately out of the top-level Cargo workspace so a stock
`cargo build` doesn't require WebKit + GTK system headers. Build it
from its own dir:

```sh
cd crates/desktop
npm install
cd src-tauri
cargo tauri dev    # or cargo tauri build for a release bundle
```

Ubuntu/Debian prereqs:

```sh
sudo apt install libwebkit2gtk-4.1-dev libssl-dev build-essential \
    curl wget file libxdo-dev libsoup-3.0-dev \
    libjavascriptcoregtk-4.1-dev
cargo install tauri-cli --version '^2'
```

## Running

```sh
# First boot mints an identity at ~/.config/bastion-app/identity.key
bastion whoami

# Register your laptop
bastion add ssh --label my-laptop --host 127.0.0.1 --user $USER --port 22
bastion list
```

## License

MIT. See `LICENSE`.
