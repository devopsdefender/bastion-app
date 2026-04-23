<script lang="ts" module>
  import type { TmuxSessionInfo } from "./tauri";
  export type SessionRow = {
    agent_origin: string;
    agent_label: string;
    info: TmuxSessionInfo;
  };
</script>

<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { api, type Agent, type Connector } from "./tauri";
  import {
    isAuthError,
    isAttestBlocked,
    isTmuxMissing,
    type ConnectorIssue,
  } from "./auth";

  let {
    connectors,
    active_key,
    refresh_token = 0,
    onSelect,
    onNew,
    onIssues,
  }: {
    connectors: Connector[];
    active_key: string | null; // `${origin}::${name}` of the active session
    /** Bumping this triggers an immediate re-fetch (used by ReauthBanner "retry"). */
    refresh_token?: number;
    onSelect: (row: SessionRow) => void;
    onNew: () => void;
    /** Called once per refresh with the full set of classified issues
     *  (re-pair / CF-Access-on-/attest / tmux-missing). */
    onIssues: (issues: ConnectorIssue[]) => void;
  } = $props();

  let rows: SessionRow[] = $state([]);
  let err: string | null = $state(null);
  let loading = $state(false);
  let timer: ReturnType<typeof setInterval> | null = null;

  async function refresh() {
    loading = true;
    err = null;
    const collected: SessionRow[] = [];
    const issues: ConnectorIssue[] = [];
    try {
      for (const c of connectors) {
        const agents = await api
          .fetch_agents(c.id)
          .catch((e) => {
            if (isAuthError(e)) {
              issues.push({ kind: "reauth", connector_id: c.id });
            }
            return [] as Agent[];
          });
        let any_ok = false;
        let any_auth_err = false;
        for (const a of agents) {
          const origin = a.hostname;
          try {
            const sessions = await api.tmux_list_sessions(origin);
            any_ok = true;
            for (const s of sessions) {
              collected.push({
                agent_origin: origin,
                agent_label: a.vm_name || a.hostname,
                info: s,
              });
            }
          } catch (e) {
            if (isAttestBlocked(e)) {
              issues.push({
                kind: "attest_blocked",
                connector_id: c.id,
                agent_origin: origin,
              });
            } else if (isTmuxMissing(e)) {
              issues.push({
                kind: "tmux_missing",
                connector_id: c.id,
                agent_origin: origin,
                agent_label: a.vm_name || a.hostname,
              });
            } else if (isAuthError(e)) {
              any_auth_err = true;
            }
          }
        }
        // Re-pair is per-connector, only when *every* attempted agent
        // auth-failed and none succeeded. A single working agent
        // disproves the "device not on allowlist" theory.
        if (!any_ok && any_auth_err) {
          issues.push({ kind: "reauth", connector_id: c.id });
        }
      }
      collected.sort((a, b) => b.info.activity_ts - a.info.activity_ts);
      rows = collected;
      onIssues(issues);
    } catch (e) {
      err = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    refresh();
    timer = setInterval(refresh, 10_000);
  });
  onDestroy(() => {
    if (timer) clearInterval(timer);
  });

  $effect(() => {
    // Re-fetch when the connector set changes, or when the parent
    // bumps refresh_token (e.g. after a Re-pair).
    connectors;
    refresh_token;
    refresh();
  });

  function key(r: SessionRow): string {
    return `${r.agent_origin}::${r.info.name}`;
  }

  function fmt_ago(epoch_sec: number): string {
    if (!epoch_sec) return "";
    const diff = Date.now() / 1000 - epoch_sec;
    if (diff < 60) return `${Math.floor(diff)}s`;
    if (diff < 3600) return `${Math.floor(diff / 60)}m`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h`;
    return `${Math.floor(diff / 86400)}d`;
  }
</script>

<section class="sessions">
  <div class="head">
    <h2>sessions</h2>
    <button class="new" onclick={onNew}>+ new</button>
  </div>

  {#if err}
    <div class="err">{err}</div>
  {/if}

  {#if rows.length === 0 && !loading}
    <div class="empty">
      No tmux sessions on any agent. Click <strong>+ new</strong> to start
      one.
    </div>
  {:else}
    <ul>
      {#each rows as r (key(r))}
        {@const kind = r.info.kind ?? "shell"}
        {@const k = key(r)}
        <li>
          <button
            class="row"
            class:active={active_key === k}
            onclick={() => onSelect(r)}
          >
            <span class="dot kind-{kind}">
              {r.info.attached_clients > 0 ? "●" : "○"}
            </span>
            <span class="body">
              <span class="name">{r.info.name}</span>
              <span class="meta">
                <span class="kind">{kind}</span>
                <span class="at">on {r.agent_label}</span>
              </span>
            </span>
            <span class="ago">{fmt_ago(r.info.activity_ts)}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .sessions {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h2 {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: #7d8590;
    margin: 8px 0 4px;
  }
  .new {
    background: #238636;
    color: white;
    border: 0;
    border-radius: 4px;
    padding: 3px 8px;
    cursor: pointer;
    font-size: 11px;
  }
  .new:hover {
    background: #2ea043;
  }
  .err {
    background: #f8514922;
    color: #f85149;
    padding: 4px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-family: ui-monospace, monospace;
  }
  .empty {
    color: #7d8590;
    font-style: italic;
    padding: 8px;
    font-size: 12px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: 0;
    background: transparent;
    color: #c9d1d9;
    cursor: pointer;
    border-radius: 4px;
    text-align: left;
  }
  .row:hover {
    background: #161b22;
  }
  .row.active {
    background: #1f6feb33;
    color: #58a6ff;
  }
  .dot {
    font-size: 10px;
    width: 12px;
    flex-shrink: 0;
  }
  .dot.kind-codex {
    color: #58a6ff;
  }
  .dot.kind-shell {
    color: #7d8590;
  }
  .body {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    display: flex;
    gap: 6px;
    font-size: 10px;
    color: #7d8590;
    font-family: ui-monospace, monospace;
  }
  .kind {
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .ago {
    font-size: 10px;
    color: #7d8590;
    font-family: ui-monospace, monospace;
    flex-shrink: 0;
  }
</style>
