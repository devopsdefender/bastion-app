<script lang="ts">
  import type { Connector, Whoami, SearchHit } from "./tauri";
  import type { ConnectorIssue } from "./auth";
  import SessionsList from "./SessionsList.svelte";
  import type { SessionRow } from "./SessionsList.svelte";
  import SearchPane from "./SearchPane.svelte";
  import ReauthBanner from "./ReauthBanner.svelte";
  import InfoBanner from "./InfoBanner.svelte";

  let {
    whoami,
    connectors,
    active_key,
    refresh_token = 0,
    issues,
    dismissed_issue_keys,
    onSelectSession,
    onNewSession,
    onAddConnector,
    onRemoveConnector,
    onSearchHit,
    onIssues,
    onDismissIssue,
    onPaired,
  }: {
    whoami: Whoami | null;
    connectors: Connector[];
    active_key: string | null;
    refresh_token?: number;
    issues: ConnectorIssue[];
    dismissed_issue_keys: Set<string>;
    onSelectSession: (row: SessionRow) => void;
    onNewSession: () => void;
    onAddConnector: () => void;
    onRemoveConnector: (id: string) => void;
    onSearchHit: (hit: SearchHit) => void;
    onIssues: (issues: ConnectorIssue[]) => void;
    onDismissIssue: (key: string) => void;
    onPaired: (connector_id: string) => void;
  } = $props();

  let nodes_expanded = $state(false);

  function issue_key(i: ConnectorIssue): string {
    if (i.kind === "reauth") return `reauth:${i.connector_id}`;
    return `${i.kind}:${i.connector_id}:${i.agent_origin}`;
  }

  function connector_by_id(id: string): Connector | undefined {
    return connectors.find((c) => c.id === id);
  }

  const visible_issues = $derived(
    issues.filter((i) => !dismissed_issue_keys.has(issue_key(i))),
  );
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

  {#each visible_issues as issue (issue_key(issue))}
    {@const c = connector_by_id(issue.connector_id)}
    {#if c}
      {#if issue.kind === "reauth"}
        <ReauthBanner
          connector={c}
          onDismiss={() => onDismissIssue(issue_key(issue))}
          onPaired={() => onPaired(c.id)}
        />
      {:else if issue.kind === "attest_blocked"}
        <InfoBanner
          title={`/attest blocked on ${issue.agent_origin}`}
          body="Cloudflare Access is intercepting the agent's /attest endpoint. It must be publicly reachable — ask your DD admin to exempt /attest from CF Access."
          onDismiss={() => onDismissIssue(issue_key(issue))}
        />
      {:else if issue.kind === "tmux_missing"}
        <InfoBanner
          title={`tmux missing on ${issue.agent_label}`}
          body="Bastion uses tmux for session persistence. Install tmux on the agent (e.g. `apt install tmux`) and retry."
          onDismiss={() => onDismissIssue(issue_key(issue))}
        />
      {/if}
    {/if}
  {/each}

  <SessionsList
    {connectors}
    {active_key}
    {refresh_token}
    onSelect={onSelectSession}
    onNew={onNewSession}
    {onIssues}
  />

  <SearchPane onSelect={onSearchHit} />

  <section class="nodes">
    <div class="nodes-toggle">
      <button
        class="nodes-expand"
        onclick={() => (nodes_expanded = !nodes_expanded)}
      >
        {nodes_expanded ? "▾" : "▸"} nodes ({connectors.length})
      </button>
      <button
        class="add-small"
        onclick={onAddConnector}
        title="add DD enclave">+</button
      >
    </div>
    {#if nodes_expanded}
      <ul>
        {#each connectors as c (c.id)}
          <li>
            <span class="conn-label">{c.label}</span>
            <button
              class="rm"
              title="remove"
              onclick={() => onRemoveConnector(c.id)}>×</button
            >
          </li>
        {/each}
        {#if connectors.length === 0}
          <li class="empty">
            No connectors yet. Click <strong>+</strong> to add a DD enclave.
          </li>
        {/if}
      </ul>
    {/if}
  </section>
</aside>

<style>
  .sidebar {
    width: 260px;
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
  .nodes {
    margin-top: auto;
    padding-top: 12px;
    border-top: 1px solid #1f242c;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .nodes-toggle {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 4px 0;
  }
  .nodes-expand {
    background: transparent;
    border: 0;
    color: #7d8590;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    cursor: pointer;
    text-align: left;
    flex: 1;
  }
  .add-small {
    background: #21262d;
    color: #c9d1d9;
    border: 1px solid #30363d;
    border-radius: 3px;
    cursor: pointer;
    padding: 0 6px;
    font-size: 12px;
    line-height: 1.4;
  }
  .add-small:hover {
    background: #30363d;
  }
  .nodes ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  .nodes li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 4px 6px;
    font-size: 12px;
  }
  .nodes li.empty {
    font-style: italic;
    color: #7d8590;
  }
  .conn-label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .rm {
    background: transparent;
    border: 0;
    color: #7d8590;
    cursor: pointer;
    padding: 0 4px;
  }
  .rm:hover {
    color: #f85149;
  }
</style>
