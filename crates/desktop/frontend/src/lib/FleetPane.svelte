<!--
  Three-column fleet view.

  [ agents ]        [ workloads ]        [ logs ]
    ○ control-plane    ○ dd-management      <tail>
    ○ dd-prod-agent-   ○ ttyd
                       ○ cloudflared

  On mount:
    - fetch_agents(connector.id) populates the first column.
  On agent select:
    - lazy-open a Noise session to agent.hostname (one per agent,
      cached in a Map keyed by hostname). Send `list` → deployments.
  On deployment select:
    - send `logs { id, tail: 200 }` on the agent's session.
    - a "refresh" button re-fetches on demand.

  Each agent's session is held in `session_ids` + `noise_pubkeys`.
  The Tauri backend owns the actual `NoiseClient` objects; we only
  hold their ids to drive `connect_send`.
-->
<script lang="ts">
  import { api } from "./tauri";
  import type { Agent, Connector, Deployment } from "./tauri";

  let { connector }: { connector: Connector } = $props();

  // First column: agents fleet
  let agents: Agent[] = $state([]);
  let agents_err: string | null = $state(null);
  let loading_agents = $state(false);

  // Active selection
  let active_agent: Agent | null = $state(null);
  let active_deployment: Deployment | null = $state(null);

  // Per-agent state (keyed by hostname)
  let session_ids: Record<string, string> = $state({});
  let noise_pubkeys: Record<string, string> = $state({});
  let connecting: Record<string, boolean> = $state({});
  let agent_err: Record<string, string> = $state({});

  // Workloads for active agent
  let deployments: Deployment[] = $state([]);
  let loading_deployments = $state(false);

  // Logs for active deployment
  let log_text = $state("");
  let log_tail = $state(200);
  let loading_logs = $state(false);
  let logs_err: string | null = $state(null);

  async function refresh_agents() {
    loading_agents = true;
    agents_err = null;
    try {
      agents = await api.fetch_agents(connector.id);
    } catch (e) {
      agents_err = String(e);
    } finally {
      loading_agents = false;
    }
  }

  async function ensure_session(agent: Agent): Promise<string | null> {
    const key = agent.hostname;
    if (session_ids[key]) return session_ids[key];
    if (connecting[key]) return null;
    connecting[key] = true;
    agent_err[key] = "";
    try {
      const r = await api.connect_start_to(agent.hostname);
      session_ids[key] = r.session_id;
      noise_pubkeys[key] = r.pubkey_hex;
      return r.session_id;
    } catch (e) {
      agent_err[key] = String(e);
      return null;
    } finally {
      connecting[key] = false;
    }
  }

  async function select_agent(agent: Agent) {
    active_agent = agent;
    active_deployment = null;
    deployments = [];
    log_text = "";
    logs_err = null;
    const sid = await ensure_session(agent);
    if (!sid) return;
    await refresh_deployments(agent, sid);
  }

  async function refresh_deployments(agent: Agent, sid: string) {
    loading_deployments = true;
    try {
      const resp = (await api.connect_send(sid, {
        method: "list",
      })) as { deployments?: Deployment[] } & Record<string, unknown>;
      // EE returns `{ deployments: [...] }`; be forgiving if the
      // shape drifts (proxy wraps errors as `{ error, detail }`).
      if (resp && Array.isArray((resp as any).deployments)) {
        deployments = (resp as any).deployments;
      } else if ((resp as any)?.error) {
        agent_err[agent.hostname] = String((resp as any).detail ?? (resp as any).error);
        deployments = [];
      } else {
        deployments = [];
      }
    } catch (e) {
      agent_err[agent.hostname] = String(e);
    } finally {
      loading_deployments = false;
    }
  }

  async function select_deployment(d: Deployment) {
    active_deployment = d;
    await refresh_logs();
  }

  async function refresh_logs() {
    if (!active_agent || !active_deployment) return;
    const sid = session_ids[active_agent.hostname];
    if (!sid) return;
    loading_logs = true;
    logs_err = null;
    try {
      const resp = (await api.connect_send(sid, {
        method: "logs",
        id: active_deployment.id,
        tail: log_tail,
      })) as { lines?: string[] } & Record<string, unknown>;
      if (resp && Array.isArray((resp as any).lines)) {
        log_text = (resp as any).lines.join("\n");
      } else if ((resp as any)?.error) {
        logs_err = String((resp as any).detail ?? (resp as any).error);
      } else {
        log_text = JSON.stringify(resp, null, 2);
      }
    } catch (e) {
      logs_err = String(e);
    } finally {
      loading_logs = false;
    }
  }

  refresh_agents();
</script>

<section class="pane">
  <!-- ── Column 1: agents ─────────────────────────────── -->
  <nav class="col agents-col">
    <header>
      <div class="label">{connector.label}</div>
      <button class="btn ghost" onclick={refresh_agents} disabled={loading_agents}>
        ↻
      </button>
    </header>
    {#if agents_err}
      <div class="err">{agents_err}</div>
    {/if}
    {#if loading_agents && !agents.length}
      <div class="dim">loading…</div>
    {:else if !agents.length}
      <div class="dim">no agents</div>
    {:else}
      <ul>
        {#each agents as a (a.agent_id)}
          {@const selected = active_agent?.agent_id === a.agent_id}
          {@const session = session_ids[a.hostname]}
          {@const connecting_now = connecting[a.hostname]}
          <li class:active={selected}>
            <button class="row" onclick={() => select_agent(a)}>
              <div class="row-top">
                <span class="vm">{a.vm_name}</span>
                <span class="status {a.status}">{a.status}</span>
              </div>
              <div class="row-bot">
                <span class="host">{a.hostname}</span>
                {#if connecting_now}
                  <span class="dot">…</span>
                {:else if session}
                  <span class="dot ok" title="noise session open">●</span>
                {/if}
              </div>
              {#if agent_err[a.hostname]}
                <div class="err-small">{agent_err[a.hostname]}</div>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </nav>

  <!-- ── Column 2: workloads ──────────────────────────── -->
  <nav class="col workloads-col">
    <header>
      <div class="label">
        {active_agent ? `workloads · ${active_agent.vm_name}` : "workloads"}
      </div>
      {#if active_agent}
        {@const sid = session_ids[active_agent.hostname]}
        <button
          class="btn ghost"
          onclick={() => sid && refresh_deployments(active_agent!, sid)}
          disabled={loading_deployments || !sid}
        >
          ↻
        </button>
      {/if}
    </header>
    {#if !active_agent}
      <div class="dim">select an agent →</div>
    {:else if loading_deployments && !deployments.length}
      <div class="dim">listing…</div>
    {:else if !deployments.length}
      <div class="dim">no workloads</div>
    {:else}
      <ul>
        {#each deployments as d (d.id)}
          {@const selected = active_deployment?.id === d.id}
          <li class:active={selected}>
            <button class="row" onclick={() => select_deployment(d)}>
              <div class="row-top">
                <span class="vm">{d.app_name ?? d.id}</span>
                {#if d.status}<span class="status {d.status}">{d.status}</span>{/if}
              </div>
              <div class="row-bot">
                <span class="host">{d.id.slice(0, 20)}</span>
              </div>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </nav>

  <!-- ── Column 3: logs ───────────────────────────────── -->
  <section class="col logs-col">
    <header>
      <div class="label">
        {active_deployment ? `logs · ${active_deployment.app_name ?? active_deployment.id}` : "logs"}
      </div>
      {#if active_deployment}
        <label class="tail-label">
          tail
          <input
            type="number"
            bind:value={log_tail}
            min="1"
            max="10000"
            onchange={refresh_logs}
          />
        </label>
        <button
          class="btn ghost"
          onclick={refresh_logs}
          disabled={loading_logs}
        >
          ↻
        </button>
      {/if}
    </header>
    {#if logs_err}
      <div class="err">{logs_err}</div>
    {/if}
    {#if !active_deployment}
      <div class="dim">select a workload →</div>
    {:else if loading_logs && !log_text}
      <div class="dim">fetching…</div>
    {:else if !log_text}
      <div class="dim">(no log output)</div>
    {:else}
      <pre class="output">{log_text}</pre>
    {/if}
  </section>
</section>

<style>
  .pane {
    flex: 1;
    display: flex;
    height: 100%;
    min-height: 0;
    background: #010409;
    color: #c9d1d9;
    font-family: ui-sans-serif, system-ui;
    font-size: 13px;
  }

  .col {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    border-right: 1px solid #1f242c;
  }
  .agents-col {
    width: 240px;
    flex-shrink: 0;
  }
  .workloads-col {
    width: 260px;
    flex-shrink: 0;
  }
  .logs-col {
    flex: 1;
    border-right: 0;
    min-width: 0;
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid #1f242c;
  }
  header .label {
    flex: 1;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: #7d8590;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 4px 0;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }
  li {
    padding: 0;
  }
  li.active .row {
    background: #1f6feb22;
    border-left-color: #58a6ff;
  }
  .row {
    width: 100%;
    text-align: left;
    background: transparent;
    color: inherit;
    border: 0;
    border-left: 3px solid transparent;
    padding: 8px 10px;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row:hover {
    background: #161b22;
  }
  .row-top,
  .row-bot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .vm {
    font-weight: 500;
  }
  .host {
    font-family: ui-monospace, monospace;
    font-size: 10px;
    color: #7d8590;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .status {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 3px;
    background: #21262d;
    color: #7d8590;
    text-transform: lowercase;
  }
  .status.healthy,
  .status.running {
    background: #23863633;
    color: #56d364;
  }
  .status.unhealthy,
  .status.stopped,
  .status.failed {
    background: #f8514933;
    color: #ff8078;
  }
  .dot {
    font-size: 12px;
    color: #7d8590;
  }
  .dot.ok {
    color: #56d364;
  }

  .dim {
    color: #7d8590;
    font-style: italic;
    padding: 10px 12px;
  }
  .err {
    background: #f8514922;
    color: #f85149;
    padding: 6px 10px;
    margin: 8px 12px;
    border-radius: 4px;
    font-family: ui-monospace, monospace;
    font-size: 11px;
    white-space: pre-wrap;
  }
  .err-small {
    font-size: 10px;
    color: #f85149;
    margin-top: 2px;
    white-space: pre-wrap;
  }

  .output {
    flex: 1;
    background: #0d1117;
    border: 0;
    padding: 10px 12px;
    margin: 0;
    overflow: auto;
    font-family: ui-monospace, monospace;
    font-size: 12px;
    white-space: pre-wrap;
  }
  .btn {
    background: transparent;
    color: #c9d1d9;
    border: 1px solid #30363d;
    padding: 2px 8px;
    border-radius: 3px;
    cursor: pointer;
    font-size: 12px;
  }
  .btn.ghost:hover {
    background: #21262d;
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .tail-label {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: #7d8590;
  }
  .tail-label input {
    width: 60px;
    background: #0d1117;
    color: #c9d1d9;
    border: 1px solid #30363d;
    padding: 2px 4px;
    border-radius: 3px;
  }
</style>
