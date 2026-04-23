<script lang="ts">
  import { api, type Connector } from "./tauri";

  let {
    connector,
    onDismiss,
    onPaired,
  }: {
    connector: Connector;
    onDismiss: () => void;
    onPaired: () => void;
  } = $props();

  let busy = $state(false);
  let instructions: string | null = $state(null);
  let err: string | null = $state(null);

  const cp_url = $derived(String(connector.config.origin ?? ""));

  async function repair() {
    busy = true;
    err = null;
    instructions = null;
    try {
      await api.pair(cp_url, connector.label);
      instructions =
        "Browser opened. Log in via CF Access, click Confirm on the enrollment page, then click Retry below.";
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }

  function retry() {
    onPaired();
  }
</script>

<div class="banner">
  <div class="row">
    <div class="body">
      <strong>re-pair {connector.label}</strong>
      <div class="sub">
        This device isn't recognized by the control plane. Common after a
        CP redeploy that lost the device allowlist.
      </div>
    </div>
    <div class="actions">
      {#if !instructions}
        <button class="repair" disabled={busy || !cp_url} onclick={repair}>
          {busy ? "opening…" : "re-pair"}
        </button>
      {:else}
        <button class="retry" onclick={retry}>retry</button>
      {/if}
      <button class="dismiss" onclick={onDismiss} title="dismiss"
        >×</button
      >
    </div>
  </div>
  {#if instructions}
    <div class="instr">{instructions}</div>
  {/if}
  {#if err}<div class="err">{err}</div>{/if}
</div>

<style>
  .banner {
    background: #d2992222;
    border: 1px solid #d2992266;
    border-radius: 4px;
    padding: 8px 10px;
    font-size: 12px;
    color: #c9d1d9;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 8px;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .body strong {
    color: #d29922;
    font-size: 12px;
  }
  .sub {
    color: #8b949e;
    font-size: 11px;
    line-height: 1.35;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }
  .repair,
  .retry {
    background: #21262d;
    border: 1px solid #30363d;
    color: #c9d1d9;
    padding: 3px 10px;
    border-radius: 4px;
    font-size: 11px;
    cursor: pointer;
  }
  .repair:hover,
  .retry:hover {
    background: #30363d;
  }
  .repair:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .dismiss {
    background: transparent;
    border: 0;
    color: #8b949e;
    cursor: pointer;
    font-size: 14px;
    padding: 0 4px;
  }
  .dismiss:hover {
    color: #c9d1d9;
  }
  .instr {
    font-size: 11px;
    color: #8b949e;
    font-family: ui-monospace, monospace;
    line-height: 1.4;
  }
  .err {
    color: #f85149;
    font-size: 11px;
    font-family: ui-monospace, monospace;
  }
</style>
