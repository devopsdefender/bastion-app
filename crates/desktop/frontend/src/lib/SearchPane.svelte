<script lang="ts">
  import { api, type SearchHit } from "./tauri";

  let {
    onSelect,
  }: {
    onSelect: (hit: SearchHit) => void;
  } = $props();

  let query: string = $state("");
  let hits: SearchHit[] = $state([]);
  let busy = $state(false);
  let err: string | null = $state(null);

  async function run() {
    const q = query.trim();
    if (!q) {
      hits = [];
      return;
    }
    busy = true;
    err = null;
    try {
      hits = await api.search_sessions(q, 50);
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }

  function on_key(e: KeyboardEvent) {
    if (e.key === "Enter") run();
    if (e.key === "Escape") {
      query = "";
      hits = [];
    }
  }
</script>

<section class="search">
  <h2>search</h2>
  <input
    type="search"
    placeholder="fts5 query (e.g. error NEAR migration)"
    bind:value={query}
    onkeydown={on_key}
  />
  {#if busy}<div class="meta">searching…</div>{/if}
  {#if err}<div class="err">{err}</div>{/if}
  {#if hits.length > 0}
    <ul>
      {#each hits as h}
        <li>
          <button class="hit" onclick={() => onSelect(h)}>
            <div class="head">
              <span class="name">{h.tmux_name}</span>
              <span class="agent">{new URL(h.agent_origin.startsWith("http") ? h.agent_origin : `https://${h.agent_origin}`).host}</span>
            </div>
            <div class="snippet">{h.snippet}</div>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .search {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  h2 {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: #7d8590;
    margin: 8px 0 4px;
  }
  input[type="search"] {
    background: #0d1117;
    border: 1px solid #30363d;
    color: #c9d1d9;
    border-radius: 4px;
    padding: 4px 8px;
    font-size: 12px;
    font-family: ui-monospace, monospace;
    width: 100%;
    box-sizing: border-box;
  }
  .meta {
    font-size: 11px;
    color: #7d8590;
    padding: 2px 4px;
  }
  .err {
    background: #f8514922;
    color: #f85149;
    padding: 4px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-family: ui-monospace, monospace;
  }
  ul {
    list-style: none;
    margin: 4px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .hit {
    width: 100%;
    border: 0;
    background: transparent;
    text-align: left;
    padding: 4px 6px;
    border-radius: 4px;
    cursor: pointer;
    color: #c9d1d9;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .hit:hover {
    background: #161b22;
  }
  .head {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
  }
  .name {
    font-weight: 600;
  }
  .agent {
    color: #7d8590;
    font-family: ui-monospace, monospace;
    font-size: 10px;
  }
  .snippet {
    font-family: ui-monospace, monospace;
    font-size: 11px;
    color: #8b949e;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
