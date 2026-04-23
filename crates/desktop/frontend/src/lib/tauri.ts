// Thin wrapper around `invoke` so the rest of the UI can call typed
// helpers rather than reaching into `@tauri-apps/api/core`.

import { invoke } from "@tauri-apps/api/core";

export interface Whoami {
  fingerprint: string;
  pubkey_hex: string;
  config_dir: string;
}

export type ConnectorKind = "dd-enclave";

export interface Connector {
  id: string;
  kind: ConnectorKind;
  label: string;
  config: Record<string, unknown>;
  created_at_ms: number;
}

export interface Agent {
  agent_id: string;
  vm_name: string;
  hostname: string;
  status: string;
  last_seen: string | null;
}

export type SessionKind = "shell" | "codex";

export interface TmuxSessionInfo {
  name: string;
  activity_ts: number;
  attached_clients: number;
  foreground_cmd: string | null;
  kind: string | null; // "shell" | "codex" | null — heuristic from foreground_cmd
}

export interface AttachResult {
  bastion_session_id: string;
  ack: unknown;
}

export interface SearchHit {
  session_id: string;
  agent_origin: string;
  tmux_name: string;
  ts_ms: number;
  snippet: string;
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
      url: string;
      cp_url: string;
      pubkey_hex: string;
      label: string;
    }>("pair", { args: { cp_url, label } }),

  fetch_agents: (connector_id: string) =>
    invoke<Agent[]>("fetch_agents", { connectorId: connector_id }),

  tmux_list_sessions: (agent_origin: string) =>
    invoke<TmuxSessionInfo[]>("tmux_list_sessions", {
      args: { agent_origin },
    }),
  tmux_new_session: (
    agent_origin: string,
    name: string,
    kind: SessionKind,
    inner_cmd?: string[] | null,
  ) =>
    invoke<AttachResult>("tmux_new_session", {
      args: { agent_origin, name, kind, inner_cmd: inner_cmd ?? null },
    }),
  tmux_attach: (agent_origin: string, name: string, kind?: string | null) =>
    invoke<AttachResult>("tmux_attach", {
      args: { agent_origin, name, kind: kind ?? null },
    }),
  tmux_write: (bastion_session_id: string, bytes_b64: string) =>
    invoke<void>("tmux_write", { args: { bastion_session_id, bytes_b64 } }),
  tmux_resize: (bastion_session_id: string, cols: number, rows: number) =>
    invoke<void>("tmux_resize", {
      args: { bastion_session_id, cols, rows },
    }),
  tmux_detach: (bastion_session_id: string) =>
    invoke<void>("tmux_detach", { args: { bastion_session_id } }),
  tmux_kill_session: (agent_origin: string, name: string) =>
    invoke<void>("tmux_kill_session", { args: { agent_origin, name } }),

  search_sessions: (query: string, limit = 50) =>
    invoke<SearchHit[]>("search_sessions", { args: { query, limit } }),
};
