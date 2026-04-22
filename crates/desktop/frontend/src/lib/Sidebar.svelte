<script lang="ts">
  import type { Connector, Whoami } from "./tauri";
  import { api } from "./tauri";

  let {
    whoami,
    connectors,
    active_id,
    onSelect,
    onAdd,
    onRemove,
  }: {
    whoami: Whoami | null;
    connectors: Connector[];
    active_id: string | null;
    onSelect: (c: Connector) => void;
    onAdd: () => void;
    onRemove: (id: string) => void;
  } = $props();

  const kinds = ["dd-enclave", "ssh-host", "anthropic", "github", "local-shell"] as const;
</script>

<aside class="sidebar">
  <header>
    <h1>bastion</h1>
    {#if whoami}
      <div class="whoami">
        <div class="fp">{whoami.fingerprint}</div>
        <div class="pk" title={whoami.pubkey_hex}>
          {whoami.pubkey_hex.slice(0, 16)}…
        </div>
      </div>
    {/if}
  </header>

  <button class="add" onclick={onAdd}>+ add connector</button>

  {#each kinds as kind}
    {@const group = connectors.filter((c) => c.kind === kind)}
    {#if group.length}
      <section>
        <h2>{kind}</h2>
        <ul>
          {#each group as c (c.id)}
            <li class:active={c.id === active_id}>
              <button class="conn" onclick={() => onSelect(c)}>
                <span class="label">{c.label}</span>
                <span class="id">{c.id.slice(0, 8)}</span>
              </button>
              <button class="rm" title="remove" onclick={() => onRemove(c.id)}>×</button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/each}

  {#if !connectors.length}
    <div class="empty">
      No connectors yet. Click <strong>+ add connector</strong> to pair with
      an enclave.
    </div>
  {/if}
</aside>

<style>
  .sidebar {
    width: 240px;
    background: #0e1116;
    color: #c9d1d9;
    border-right: 1px solid #1f242c;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow-y: auto;
    font-family: ui-sans-serif, system-ui;
    font-size: 13px;
  }
  header {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-bottom: 12px;
    border-bottom: 1px solid #1f242c;
  }
  h1 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .whoami {
    display: flex;
    justify-content: space-between;
    font-family: ui-monospace, monospace;
    font-size: 11px;
    color: #7d8590;
  }
  .add {
    background: #238636;
    color: white;
    border: 0;
    border-radius: 4px;
    padding: 6px 10px;
    cursor: pointer;
    font-size: 12px;
  }
  section h2 {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: #7d8590;
    margin: 8px 0 4px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  li {
    display: flex;
    align-items: center;
  }
  li .conn {
    flex: 1;
    text-align: left;
    padding: 6px 8px;
    border: 0;
    background: transparent;
    color: inherit;
    cursor: pointer;
    border-radius: 4px;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  li.active .conn {
    background: #1f6feb33;
    color: #58a6ff;
  }
  li .conn:hover {
    background: #161b22;
  }
  .id {
    font-family: ui-monospace, monospace;
    font-size: 10px;
    color: #7d8590;
  }
  .rm {
    background: transparent;
    border: 0;
    color: #7d8590;
    cursor: pointer;
    padding: 0 6px;
  }
  .rm:hover {
    color: #f85149;
  }
  .empty {
    color: #7d8590;
    font-style: italic;
    padding: 8px;
  }
</style>
