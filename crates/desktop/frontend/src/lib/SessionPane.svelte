<script lang="ts">
  import type { Connector } from "./tauri";
  import { api } from "./tauri";

  let { connector }: { connector: Connector } = $props();

  let session_id: string | null = $state(null);
  let pubkey_hex: string | null = $state(null);
  let verified = $state(false);
  let connecting = $state(false);
  let err: string | null = $state(null);

  let output: unknown = $state(null);
  let sending = $state(false);
  let method = $state("health");
  let deployment = $state("");
  let tail = $state(200);

  // Parent uses `{#key connector.id}` so this component remounts on
  // connector change — local state is fresh per connector with no
  // in-component reset logic. An earlier `$effect` here wiped
  // `session_id` on self-writes, which made Connect appear silent.

  async function open_session() {
    connecting = true;
    err = null;
    try {
      const r = await api.connect_start(connector.id);
      session_id = r.session_id;
      pubkey_hex = r.pubkey_hex;
      verified = r.verified;
    } catch (e) {
      err = String(e);
    } finally {
      connecting = false;
    }
  }

  async function send() {
    if (!session_id) return;
    sending = true;
    err = null;
    output = null;
    try {
      let request: Record<string, unknown> = { method };
      if (method === "logs") {
        request.id = deployment;
        request.tail = tail;
      }
      output = await api.connect_send(session_id, request);
    } catch (e) {
      err = String(e);
    } finally {
      sending = false;
    }
  }

  async function close_session() {
    if (!session_id) return;
    try {
      await api.connect_close(session_id);
    } catch (e) {
      err = String(e);
    }
    session_id = null;
    pubkey_hex = null;
    verified = false;
    output = null;
  }
</script>

<section class="pane">
  <header>
    <div class="title">
      <h2>{connector.label}</h2>
      <span class="kind">{connector.kind}</span>
    </div>
    {#if pubkey_hex}
      <div class="pin" title={pubkey_hex}>
        pinned pubkey: {pubkey_hex.slice(0, 16)}…
        {#if !verified}<span class="tofu">TOFU</span>{/if}
      </div>
    {/if}
  </header>

  {#if !session_id}
    <div class="connect-box">
      <p>Open a Noise_IK session to this enclave.</p>
      <button onclick={open_session} disabled={connecting}>
        {connecting ? "handshaking…" : "connect"}
      </button>
    </div>
  {:else}
    <div class="toolbar">
      <label>
        method
        <select bind:value={method}>
          <option value="health">health</option>
          <option value="list">list</option>
          <option value="logs">logs</option>
        </select>
      </label>
      {#if method === "logs"}
        <label>
          deployment
          <input bind:value={deployment} placeholder="deployment id" />
        </label>
        <label>
          tail
          <input type="number" bind:value={tail} min="1" max="10000" />
        </label>
      {/if}
      <button onclick={send} disabled={sending || (method === "logs" && !deployment)}>
        {sending ? "…" : "send"}
      </button>
      <button class="close" onclick={close_session}>close session</button>
    </div>

    <pre class="output">{output === null ? "(no response yet)" : JSON.stringify(output, null, 2)}</pre>
  {/if}

  {#if err}<div class="err">{err}</div>{/if}
</section>

<style>
  .pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: #010409;
    color: #c9d1d9;
    padding: 14px;
    gap: 12px;
    overflow: hidden;
    font-family: ui-sans-serif, system-ui;
    font-size: 13px;
  }
  header {
    display: flex;
    flex-direction: column;
    gap: 4px;
    border-bottom: 1px solid #1f242c;
    padding-bottom: 10px;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  h2 {
    margin: 0;
    font-size: 15px;
  }
  .kind {
    font-family: ui-monospace, monospace;
    font-size: 11px;
    color: #7d8590;
    padding: 2px 6px;
    border: 1px solid #30363d;
    border-radius: 3px;
  }
  .pin {
    font-family: ui-monospace, monospace;
    font-size: 11px;
    color: #7d8590;
  }
  .tofu {
    color: #d29922;
    margin-left: 6px;
    font-weight: 600;
  }
  .connect-box {
    padding: 24px;
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 6px;
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .toolbar {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    flex-wrap: wrap;
  }
  .toolbar label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 11px;
    color: #7d8590;
  }
  .toolbar input,
  .toolbar select {
    background: #0d1117;
    color: #c9d1d9;
    border: 1px solid #30363d;
    padding: 4px 6px;
    border-radius: 3px;
  }
  button {
    padding: 6px 12px;
    background: #238636;
    color: white;
    border: 0;
    border-radius: 4px;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .close {
    background: #21262d;
    color: #c9d1d9;
    border: 1px solid #30363d;
  }
  .output {
    flex: 1;
    background: #0d1117;
    border: 1px solid #1f242c;
    border-radius: 4px;
    padding: 10px;
    overflow: auto;
    font-family: ui-monospace, monospace;
    font-size: 12px;
    margin: 0;
  }
  .err {
    background: #f8514922;
    color: #f85149;
    padding: 6px 10px;
    border-radius: 4px;
    font-family: ui-monospace, monospace;
    font-size: 12px;
    white-space: pre-wrap;
  }
</style>
