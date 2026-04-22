<script lang="ts">
  import Sidebar from "./lib/Sidebar.svelte";
  import SessionPane from "./lib/SessionPane.svelte";
  import AddConnector from "./lib/AddConnector.svelte";
  import type { Connector, Whoami } from "./lib/tauri";
  import { api } from "./lib/tauri";

  let whoami: Whoami | null = $state(null);
  let connectors: Connector[] = $state([]);
  let active_id: string | null = $state(null);
  let showAdd = $state(false);
  let boot_err: string | null = $state(null);

  async function boot() {
    try {
      whoami = await api.whoami();
      connectors = await api.list_connectors();
    } catch (e) {
      boot_err = String(e);
    }
  }

  async function refresh_connectors() {
    connectors = await api.list_connectors();
  }

  async function handle_remove(id: string) {
    try {
      await api.remove_connector(id);
      if (active_id === id) active_id = null;
      await refresh_connectors();
    } catch (e) {
      boot_err = String(e);
    }
  }

  const active = $derived(
    active_id ? connectors.find((c) => c.id === active_id) ?? null : null,
  );

  boot();
</script>

<main>
  <Sidebar
    {whoami}
    {connectors}
    {active_id}
    onSelect={(c) => (active_id = c.id)}
    onAdd={() => (showAdd = true)}
    onRemove={handle_remove}
  />
  {#if active}
    <SessionPane connector={active} />
  {:else}
    <section class="empty-pane">
      <h1>bastion</h1>
      <p>Pick a connector on the left — or add one with <kbd>+</kbd>.</p>
      {#if boot_err}<div class="err">{boot_err}</div>{/if}
    </section>
  {/if}

  {#if showAdd}
    <AddConnector
      onAdded={async (c) => {
        showAdd = false;
        await refresh_connectors();
        active_id = c.id;
      }}
      onCancel={() => (showAdd = false)}
    />
  {/if}
</main>

<style>
  :global(html, body) {
    margin: 0;
    padding: 0;
    height: 100%;
    background: #010409;
  }
  :global(#app) {
    height: 100%;
  }
  main {
    display: flex;
    height: 100vh;
    width: 100vw;
  }
  .empty-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: #7d8590;
    font-family: ui-sans-serif, system-ui;
    gap: 8px;
  }
  .empty-pane h1 {
    color: #c9d1d9;
    font-size: 20px;
    margin: 0;
  }
  kbd {
    background: #21262d;
    border: 1px solid #30363d;
    padding: 1px 6px;
    border-radius: 3px;
    font-family: ui-monospace, monospace;
  }
  .err {
    background: #f8514922;
    color: #f85149;
    padding: 6px 10px;
    border-radius: 4px;
    font-family: ui-monospace, monospace;
    font-size: 12px;
    margin-top: 12px;
  }
</style>
