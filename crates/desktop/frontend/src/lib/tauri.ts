// Thin wrapper around `invoke` so the rest of the UI can call typed
// helpers rather than reaching into `@tauri-apps/api/core`.

import { invoke } from "@tauri-apps/api/core";

export interface Whoami {
  fingerprint: string;
  pubkey_hex: string;
  config_dir: string;
}

export type ConnectorKind =
  | "dd-enclave"
  | "ssh-host"
  | "anthropic"
  | "github"
  | "local-shell";

export interface Connector {
  id: string;
  kind: ConnectorKind;
  label: string;
  config: Record<string, unknown>;
  created_at_ms: number;
}

export const api = {
  whoami: () => invoke<Whoami>("whoami"),
  list_connectors: () => invoke<Connector[]>("list_connectors"),
  add_connector: (
    kind: ConnectorKind,
    label: string,
    config: Record<string, unknown>,
  ) => invoke<Connector>("add_connector", { args: { kind, label, config } }),
  remove_connector: (id: string) => invoke<void>("remove_connector", { id }),
  pair: (cp_url: string, label?: string) =>
    invoke<{
      status: number;
      pubkey: string;
      label: string;
      body: unknown;
    }>("pair", { args: { cp_url, label } }),

  connect_start: (connector_id: string) =>
    invoke<{
      session_id: string;
      origin: string;
      pubkey_hex: string;
      verified: boolean;
    }>("connect_start", { args: { connector_id } }),
  connect_send: (session_id: string, request: unknown) =>
    invoke<unknown>("connect_send", { args: { session_id, request } }),
  connect_close: (session_id: string) =>
    invoke<void>("connect_close", { args: { session_id } }),
};
