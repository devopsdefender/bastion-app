<script>
  let { me, connectors, selected, onselect, onadd, onremove } = $props();

  let adding = $state(false);
  let label = $state("");
  let origin = $state("");

  function submit(e) {
    e.preventDefault();
    if (!label || !origin) return;
    onadd(label, origin);
    label = "";
    origin = "";
    adding = false;
  }
</script>

<aside>
  <header>
    <h1>bastion</h1>
    {#if me}
      <p class="device">device {me.fingerprint}</p>
    {/if}
  </header>

  <ul>
    {#each connectors as c (c.id)}
      <li class={selected?.id === c.id ? "active" : ""}>
        <button type="button" onclick={() => onselect(c)}>
          <span class="label">{c.label}</span>
          <span class="kind">{c.kind}</span>
        </button>
        <button
          type="button"
          class="rm"
          title="remove"
          onclick={() => onremove(c.id)}>×</button>
      </li>
    {/each}
    {#if connectors.length === 0}
      <li class="empty">(no connectors yet)</li>
    {/if}
  </ul>

  <footer>
    {#if adding}
      <form onsubmit={submit}>
        <input bind:value={label} placeholder="label" autofocus />
        <input
          bind:value={origin}
          placeholder="https://block.pr-N.devopsdefender.com" />
        <div class="row">
          <button type="submit">add</button>
          <button type="button" onclick={() => (adding = false)}>cancel</button>
        </div>
      </form>
    {:else}
      <button type="button" class="plus" onclick={() => (adding = true)}>
        + DD enclave
      </button>
    {/if}
  </footer>
</aside>

<style>
  aside {
    display: flex;
    flex-direction: column;
    background: #181825;
    border-right: 1px solid #313244;
    padding: 16px;
    overflow: hidden;
  }
  h1 {
    font-size: 18px;
    margin: 0;
  }
  .device {
    font-family: ui-monospace, monospace;
    font-size: 11px;
    color: #6c7086;
    margin: 4px 0 16px;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    flex: 1;
    overflow: auto;
  }
  li {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-bottom: 2px;
  }
  li.active button:not(.rm) {
    background: #313244;
  }
  li.empty {
    color: #6c7086;
    font-size: 13px;
    padding: 8px 10px;
  }
  li button:not(.rm) {
    flex: 1;
    text-align: left;
    background: transparent;
    color: inherit;
    border: none;
    padding: 8px 10px;
    border-radius: 4px;
    cursor: pointer;
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }
  li button:not(.rm):hover {
    background: #292a3a;
  }
  .kind {
    font-size: 11px;
    color: #6c7086;
    align-self: center;
  }
  .rm {
    background: transparent;
    color: #6c7086;
    border: none;
    cursor: pointer;
    font-size: 14px;
    padding: 0 6px;
  }
  .rm:hover {
    color: #f38ba8;
  }
  footer {
    margin-top: 16px;
    padding-top: 12px;
    border-top: 1px solid #313244;
  }
  .plus {
    width: 100%;
    background: transparent;
    color: #89b4fa;
    border: 1px dashed #45475a;
    padding: 8px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 13px;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  input {
    background: #11111b;
    color: inherit;
    border: 1px solid #45475a;
    padding: 6px 8px;
    border-radius: 4px;
    font-size: 13px;
  }
  .row {
    display: flex;
    gap: 6px;
  }
  .row button {
    flex: 1;
    background: #313244;
    color: inherit;
    border: none;
    padding: 6px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 13px;
  }
  .row button[type="submit"] {
    background: #89b4fa;
    color: #11111b;
  }
</style>
