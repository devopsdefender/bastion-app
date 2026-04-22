<script>
  import { invoke } from "@tauri-apps/api/core";
  import Sidebar from "./lib/Sidebar.svelte";

  // Top-level UI state. Svelte 5 runes.
  let me = $state(null);
  let connectors = $state([]);
  let selected = $state(null);
  let err = $state(null);

  async function refresh() {
    try {
      me = await invoke("whoami");
      connectors = await invoke("list_connectors");
    } catch (e) {
      err = String(e);
    }
  }

  async function addDdEnclave(label, origin) {
    try {
      await invoke("add_dd_enclave", { label, origin });
      await refresh();
    } catch (e) {
      err = String(e);
    }
  }

  async function removeConnector(id) {
    try {
      await invoke("remove_connector", { id });
      if (selected?.id === id) selected = null;
      await refresh();
    } catch (e) {
      err = String(e);
    }
  }

  refresh();
</script>

<div class="layout">
  <Sidebar
    {me}
    {connectors}
    {selected}
    onselect={(c) => (selected = c)}
    onadd={addDdEnclave}
    onremove={removeConnector}
  />
  <main class="pane">
    {#if err}
      <pre class="err">{err}</pre>
    {:else if selected}
      <h2>{selected.label}</h2>
      <p class="meta">kind: {selected.kind} · id: {selected.id}</p>
      <pre>{JSON.stringify(selected.config, null, 2)}</pre>
      <p class="hint">
        Session launch lands in the next milestone. Connector management works today.
      </p>
    {:else}
      <p class="hint">Select a connector, or add one via the sidebar.</p>
    {/if}
  </main>
</div>

<style>
  :global(body) {
    margin: 0;
    font-family: ui-sans-serif, system-ui, -apple-system, sans-serif;
    background: #11111b;
    color: #cdd6f4;
  }
  .layout {
    display: grid;
    grid-template-columns: 280px 1fr;
    height: 100vh;
  }
  .pane {
    padding: 24px;
    overflow: auto;
  }
  .meta {
    color: #6c7086;
    font-size: 13px;
  }
  pre {
    background: #1e1e2e;
    padding: 12px;
    border-radius: 6px;
    overflow: auto;
  }
  .err {
    color: #f38ba8;
  }
  .hint {
    color: #6c7086;
    font-size: 14px;
  }
</style>
