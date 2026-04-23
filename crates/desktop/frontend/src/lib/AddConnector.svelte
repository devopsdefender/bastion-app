<script lang="ts">
  import type { Connector } from "./tauri";
  import { api } from "./tauri";

  let {
    onAdded,
    onCancel,
  }: {
    onAdded: (c: Connector) => void;
    onCancel: () => void;
  } = $props();

  let label = $state("");
  let origin = $state("");

  // pair flow
  let cp_url = $state("https://app.devopsdefender.com");
  let pairing = $state(false);
  let pair_result: string | null = $state(null);

  let busy = $state(false);
  let err: string | null = $state(null);

  async function pair_now() {
    pairing = true;
    err = null;
    pair_result = null;
    try {
      const r = await api.pair(cp_url, label || undefined);
      pair_result =
        `opened ${r.url} in your browser — log in (CF Access) and ` +
        `click Confirm on the enrollment page. When it says "Enrolled ✓", ` +
        `close the tab and add the enclave origin below.`;
    } catch (e) {
      err = String(e);
    } finally {
      pairing = false;
    }
  }

  async function submit(e: Event) {
    e.preventDefault();
    busy = true;
    err = null;
    try {
      const c = await api.add_connector("dd-enclave", label, { origin });
      onAdded(c);
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div
  class="overlay"
  onclick={onCancel}
  onkeydown={(e) => e.key === "Escape" && onCancel()}
  role="button"
  tabindex="-1"
>
  <form
    class="modal"
    onclick={(e) => e.stopPropagation()}
    onsubmit={submit}
  >
    <h2>Add DD enclave</h2>

    <label>
      <span>Label</span>
      <input bind:value={label} placeholder="prod / staging / …" required />
    </label>

    <fieldset>
      <legend>Pair with DD control plane</legend>
      <label>
        <span>CP URL</span>
        <input
          bind:value={cp_url}
          placeholder="https://app.devopsdefender.com"
        />
      </label>
      <button type="button" onclick={pair_now} disabled={pairing}>
        {pairing ? "pairing…" : "pair this device"}
      </button>
      {#if pair_result}<div class="ok">{pair_result}</div>{/if}
    </fieldset>

    <label>
      <span>Origin</span>
      <input
        bind:value={origin}
        placeholder="https://app.devopsdefender.com"
        required
      />
    </label>

    {#if err}<div class="err">{err}</div>{/if}

    <div class="actions">
      <button type="button" onclick={onCancel}>cancel</button>
      <button type="submit" disabled={busy || !label || !origin}>
        {busy ? "saving…" : "save"}
      </button>
    </div>
  </form>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(1, 4, 9, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .modal {
    background: #0d1117;
    color: #c9d1d9;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 16px;
    min-width: 380px;
    max-width: 460px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    font-family: ui-sans-serif, system-ui;
    font-size: 13px;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 15px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  label span {
    font-size: 11px;
    color: #7d8590;
    text-transform: uppercase;
  }
  input {
    padding: 6px 8px;
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 4px;
    color: #c9d1d9;
    font-family: inherit;
  }
  fieldset {
    border: 1px dashed #30363d;
    border-radius: 4px;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  legend {
    color: #7d8590;
    padding: 0 4px;
    font-size: 11px;
    text-transform: uppercase;
  }
  button {
    padding: 6px 12px;
    border: 1px solid #30363d;
    background: #21262d;
    color: #c9d1d9;
    border-radius: 4px;
    cursor: pointer;
  }
  button[type="submit"] {
    background: #238636;
    border-color: #238636;
    color: white;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
  .err {
    background: #f8514922;
    color: #f85149;
    padding: 6px 10px;
    border-radius: 4px;
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }
  .ok {
    background: #23863622;
    color: #2ea043;
    padding: 6px 10px;
    border-radius: 4px;
    font-size: 12px;
  }
</style>
