<script lang="ts">
  // Non-actionable warning banner. For issues whose remediation is
  // outside bastion's reach (server-side CF Access policy, admin
  // installing tmux on an agent). If the issue becomes actionable
  // later, swap in a richer banner.

  let {
    title,
    body,
    onDismiss,
  }: {
    title: string;
    body: string;
    onDismiss?: () => void;
  } = $props();
</script>

<div class="banner">
  <div class="body">
    <strong>{title}</strong>
    <div class="sub">{body}</div>
  </div>
  {#if onDismiss}
    <button class="dismiss" onclick={onDismiss} title="dismiss">×</button>
  {/if}
</div>

<style>
  .banner {
    background: #1f6feb22;
    border: 1px solid #1f6feb44;
    border-radius: 4px;
    padding: 8px 10px;
    font-size: 12px;
    color: #c9d1d9;
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 8px;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .body strong {
    color: #58a6ff;
    font-size: 12px;
  }
  .sub {
    color: #8b949e;
    font-size: 11px;
    line-height: 1.35;
  }
  .dismiss {
    background: transparent;
    border: 0;
    color: #8b949e;
    cursor: pointer;
    font-size: 14px;
    padding: 0 4px;
    flex-shrink: 0;
  }
  .dismiss:hover {
    color: #c9d1d9;
  }
</style>
