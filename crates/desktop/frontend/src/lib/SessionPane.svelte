<script lang="ts">
  import TerminalView from "./TerminalView.svelte";
  import { api } from "./tauri";
  import type { SessionKind } from "./tauri";

  // Mode: either we're attaching to an existing tmux session, or
  // we're about to create one. Either path resolves to an attach on
  // the agent side — the backend command differs.
  let {
    agent_origin,
    tmux_name,
    mode,
    kind,
    onClosed,
  }: {
    agent_origin: string;
    tmux_name: string;
    mode: "attach" | "new";
    kind: SessionKind;
    onClosed?: () => void;
  } = $props();

  let detaching = $state(false);
  let killing = $state(false);
  let killErr: string | null = $state(null);

  const source = {
    spawn: async () => {
      const r =
        mode === "new"
          ? await api.tmux_new_session(agent_origin, tmux_name, kind, null)
          : await api.tmux_attach(agent_origin, tmux_name, kind);
      return { sessionId: r.bastion_session_id };
    },
    write: (id: string, b64: string) => api.tmux_write(id, b64),
    resize: (id: string, cols: number, rows: number) =>
      api.tmux_resize(id, cols, rows),
    close: (id: string) => api.tmux_detach(id),
    eventPrefix: (id: string) => `session:${id}`,
  };

  async function kill_session() {
    killing = true;
    killErr = null;
    try {
      await api.tmux_kill_session(agent_origin, tmux_name);
      onClosed?.();
    } catch (e) {
      killErr = String(e);
    } finally {
      killing = false;
    }
  }

  async function detach_and_close() {
    detaching = true;
    try {
      onClosed?.();
    } finally {
      detaching = false;
    }
  }
</script>

<section class="wrap">
  <header class="head">
    <div class="ident">
      <span class="kind {kind}">{kind}</span>
      <span class="name">{tmux_name}</span>
      <span class="agent">@ {agent_origin}</span>
    </div>
    <div class="actions">
      <button class="btn" disabled={detaching} onclick={detach_and_close}
        >detach</button
      >
      <button
        class="btn danger"
        disabled={killing}
        onclick={kill_session}
        title="kill this tmux session on the agent">kill</button
      >
    </div>
  </header>
  {#if killErr}<div class="err">{killErr}</div>{/if}
  <TerminalView {source} onExit={() => onClosed?.()} />
</section>

<style>
  .wrap {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 12px;
    background: #0e1116;
    border-bottom: 1px solid #1f242c;
    color: #c9d1d9;
    font-family: ui-sans-serif, system-ui;
    font-size: 12px;
  }
  .ident {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .kind {
    padding: 1px 6px;
    border-radius: 3px;
    font-family: ui-monospace, monospace;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .kind.shell {
    background: #30363d;
    color: #c9d1d9;
  }
  .kind.codex {
    background: #1f6feb33;
    color: #58a6ff;
  }
  .name {
    font-weight: 600;
  }
  .agent {
    color: #7d8590;
    font-family: ui-monospace, monospace;
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .actions {
    display: flex;
    gap: 6px;
  }
  .btn {
    background: #21262d;
    border: 1px solid #30363d;
    color: #c9d1d9;
    padding: 3px 10px;
    border-radius: 4px;
    font-size: 11px;
    cursor: pointer;
  }
  .btn:hover {
    background: #30363d;
  }
  .btn.danger:hover {
    background: #f8514922;
    border-color: #f8514966;
    color: #f85149;
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .err {
    background: #f8514922;
    color: #f85149;
    padding: 4px 10px;
    font-family: ui-monospace, monospace;
    font-size: 11px;
  }
</style>
