<script lang="ts">
  import Sidebar from "./lib/Sidebar.svelte";
  import SessionPane from "./lib/SessionPane.svelte";
  import AddConnector from "./lib/AddConnector.svelte";
  import NewSessionDialog from "./lib/NewSessionDialog.svelte";
  import type { Connector, Whoami, SearchHit, SessionKind } from "./lib/tauri";
  import type { SessionRow } from "./lib/SessionsList.svelte";
  import { api } from "./lib/tauri";

  type ActiveSession = {
    agent_origin: string;
    tmux_name: string;
    mode: "new" | "attach";
    kind: SessionKind;
  };

  let whoami: Whoami | null = $state(null);
  let connectors: Connector[] = $state([]);
  let active: ActiveSession | null = $state(null);
  let showAdd = $state(false);
  let showNewSession = $state(false);
  let boot_err: string | null = $state(null);
  let auth_issues: Set<string> = $state(new Set());
  let dismissed_auth: Set<string> = $state(new Set());
  let sessions_refresh_token = $state(0);

  const visible_auth_issues = $derived(
    new Set(
      Array.from(auth_issues).filter((id) => !dismissed_auth.has(id)),
    ),
  );

  function on_connector_auth_state(
    connector_id: string,
    needs_auth: boolean,
  ) {
    // Keep the two sets in sync without mutating in place (svelte 5
    // reactivity on Set depends on reassignment).
    const next = new Set(auth_issues);
    if (needs_auth) next.add(connector_id);
    else next.delete(connector_id);
    auth_issues = next;
    // If the user explicitly dismissed a banner, clear that sticky
    // dismissal once the underlying problem actually resolved.
    if (!needs_auth && dismissed_auth.has(connector_id)) {
      const nd = new Set(dismissed_auth);
      nd.delete(connector_id);
      dismissed_auth = nd;
    }
  }

  function dismiss_auth_issue(connector_id: string) {
    const nd = new Set(dismissed_auth);
    nd.add(connector_id);
    dismissed_auth = nd;
  }

  function on_paired(_connector_id: string) {
    // Optimistically clear the banner and trigger a fresh fan-out —
    // if enrollment didn't actually go through, the refresh will
    // re-report the auth error and the banner returns.
    sessions_refresh_token += 1;
  }

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

  async function handle_remove_connector(id: string) {
    try {
      await api.remove_connector(id);
      await refresh_connectors();
    } catch (e) {
      boot_err = String(e);
    }
  }

  function select_session(row: SessionRow) {
    active = {
      agent_origin: row.agent_origin,
      tmux_name: row.info.name,
      mode: "attach",
      kind: (row.info.kind as SessionKind) ?? "shell",
    };
  }

  function launch_new(agent_origin: string, name: string, kind: SessionKind) {
    showNewSession = false;
    active = {
      agent_origin,
      tmux_name: name,
      mode: "new",
      kind,
    };
  }

  function on_search_hit(hit: SearchHit) {
    // Selecting a search hit attaches to the session it came from.
    active = {
      agent_origin: hit.agent_origin,
      tmux_name: hit.tmux_name,
      mode: "attach",
      kind: "shell",
    };
  }

  const active_key = $derived(
    active ? `${active.agent_origin}::${active.tmux_name}` : null,
  );

  boot();
</script>

<main>
  <Sidebar
    {whoami}
    {connectors}
    {active_key}
    refresh_token={sessions_refresh_token}
    auth_issues={visible_auth_issues}
    onSelectSession={select_session}
    onNewSession={() => (showNewSession = true)}
    onAddConnector={() => (showAdd = true)}
    onRemoveConnector={handle_remove_connector}
    onSearchHit={on_search_hit}
    onConnectorAuthState={on_connector_auth_state}
    onDismissAuthIssue={dismiss_auth_issue}
    onPaired={on_paired}
  />

  {#if active}
    {#key active_key}
      <SessionPane
        agent_origin={active.agent_origin}
        tmux_name={active.tmux_name}
        mode={active.mode}
        kind={active.kind}
        onClosed={() => (active = null)}
      />
    {/key}
  {:else}
    <section class="empty-pane">
      <h1>bastion</h1>
      <p>
        Pick a session on the left, or click <kbd>+ new</kbd> to start one.
      </p>
      {#if boot_err}<div class="err">{boot_err}</div>{/if}
    </section>
  {/if}

  {#if showAdd}
    <AddConnector
      onAdded={async () => {
        showAdd = false;
        await refresh_connectors();
      }}
      onCancel={() => (showAdd = false)}
    />
  {/if}

  {#if showNewSession}
    <NewSessionDialog
      {connectors}
      onLaunch={launch_new}
      onCancel={() => (showNewSession = false)}
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
