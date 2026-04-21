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

Milestone 0 — scaffolding. What works today:

- `bastion whoami` — device identity fingerprint + X25519 pubkey
- `bastion list` — dump the connector store
- `bastion add ssh|dd-enclave|anthropic`
- `bastion rm <id>`

Next milestones:

- `bastion connect <id>` — actually open a session (SSH first, DD
  enclave next, local PTY after that)
- OSC 133 block parser wired into session stdout
- ratatui TUI showing sidebar + active session
- Tauri v2 desktop wrapper around a shared Svelte frontend
- Signal-style multi-device sync (identity seed as the pairing root)

## Layout

```
crates/
├── core/       — shared types (identity, connector, store)
├── cli/        — `bastion` binary, first front-end
└── desktop/    — Tauri v2 shell (added in a follow-up)
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
