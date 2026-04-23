<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Agent, type Connector, type SessionKind } from "./tauri";

  type AgentRow = { connector_id: string; connector_label: string; agent: Agent };

  let {
    connectors,
    onLaunch,
    onCancel,
  }: {
    connectors: Connector[];
    onLaunch: (
      agent_origin: string,
      name: string,
      kind: SessionKind,
    ) => void;
    onCancel: () => void;
  } = $props();

  let agents: AgentRow[] = $state([]);
  let agent_origin: string = $state("");
  let kind: SessionKind = $state("shell");
  let name: string = $state("");
  let err: string | null = $state(null);
  let loading = $state(true);

  onMount(async () => {
    const collected: AgentRow[] = [];
    for (const c of connectors) {
      try {
        const list = await api.fetch_agents(c.id);
        for (const a of list) {
          collected.push({
            connector_id: c.id,
            connector_label: c.label,
            agent: a,
          });
        }
      } catch (e) {
        err = String(e);
      }
    }
    agents = collected;
    if (agents.length > 0) {
      agent_origin = agents[0].agent.hostname;
    }
    // Default name: "$USER-$timestamp" stripped of odd chars.
    const ts = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    name = `s-${pad(ts.getHours())}${pad(ts.getMinutes())}`;
    loading = false;
  });

  function submit() {
    if (!agent_origin || !name.trim()) return;
    // tmux session names can't contain a colon or dot.
    const safe = name.trim().replace(/[:.]/g, "-");
    onLaunch(agent_origin, safe, kind);
  }
</script>

<div class="backdrop" role="dialog" aria-modal="true">
  <div class="modal">
    <h2>New session</h2>

    {#if loading}
      <p class="meta">Loading agents…</p>
    {:else if agents.length === 0}
      <p class="err">
        No agents available. Make sure a DD connector is paired and reachable.
      </p>
    {:else}
      <label>
        <span>Agent</span>
        <select bind:value={agent_origin}>
          {#each agents as row}
            <option value={row.agent.hostname}>
              {row.agent.vm_name || row.agent.hostname} ({row.connector_label})
            </option>
          {/each}
        </select>
      </label>

      <label>
        <span>Kind</span>
        <div class="radio">
          <label class="radio-opt">
            <input type="radio" bind:group={kind} value="shell" />
            shell ($SHELL)
          </label>
          <label class="radio-opt">
            <input type="radio" bind:group={kind} value="codex" />
            codex (claude)
          </label>
        </div>
      </label>

      <label>
        <span>Name</span>
        <input type="text" bind:value={name} placeholder="session-name" />
      </label>
    {/if}

    {#if err}<div class="err">{err}</div>{/if}

    <footer>
      <button class="cancel" onclick={onCancel}>cancel</button>
      <button
        class="launch"
        disabled={loading || !agent_origin || !name.trim()}
        onclick={submit}>launch</button
      >
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: #00000088;
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .modal {
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 20px;
    width: 400px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    color: #c9d1d9;
    font-family: ui-sans-serif, system-ui;
    font-size: 13px;
  }
  h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
  }
  label > span {
    color: #7d8590;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-size: 10px;
  }
  .radio {
    display: flex;
    gap: 12px;
  }
  .radio-opt {
    flex-direction: row;
    align-items: center;
    gap: 4px;
    font-size: 13px;
    color: #c9d1d9;
    text-transform: none;
    letter-spacing: normal;
  }
  select,
  input[type="text"] {
    background: #0d1117;
    border: 1px solid #30363d;
    color: #c9d1d9;
    border-radius: 4px;
    padding: 6px 8px;
    font-size: 13px;
    font-family: ui-sans-serif, system-ui;
  }
  input[type="text"] {
    font-family: ui-monospace, monospace;
  }
  .err {
    background: #f8514922;
    color: #f85149;
    padding: 6px 10px;
    border-radius: 4px;
    font-size: 12px;
    font-family: ui-monospace, monospace;
  }
  .meta {
    color: #7d8590;
    margin: 0;
    font-size: 12px;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
  .cancel,
  .launch {
    padding: 6px 12px;
    border-radius: 4px;
    border: 1px solid #30363d;
    cursor: pointer;
    font-size: 13px;
  }
  .cancel {
    background: transparent;
    color: #c9d1d9;
  }
  .launch {
    background: #238636;
    color: white;
    border-color: #238636;
  }
  .launch:hover {
    background: #2ea043;
  }
  .launch:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
