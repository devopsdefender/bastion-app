<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import "@xterm/xterm/css/xterm.css";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";

  // Parameterized terminal. Backend-agnostic: the caller supplies
  // spawn/write/resize/close + a per-session event prefix. Used for
  // tmux attach today; a future local-PTY or SSH source drops in by
  // passing a different data source.
  type DataSource = {
    spawn: (
      cols: number,
      rows: number,
    ) => Promise<{ sessionId: string }>;
    write: (sessionId: string, bytesB64: string) => Promise<void>;
    resize: (
      sessionId: string,
      cols: number,
      rows: number,
    ) => Promise<void>;
    /** Called on unmount. For tmux this is detach (session survives). */
    close: (sessionId: string) => Promise<void>;
    /** Events live on `${eventPrefix(sessionId)}:data|exit`. */
    eventPrefix: (sessionId: string) => string;
  };

  let {
    source,
    onExit,
  }: {
    source: DataSource;
    onExit?: () => void;
  } = $props();

  let host: HTMLDivElement;
  let term: Terminal | null = null;
  let fit: FitAddon | null = null;
  let sessionId: string | null = null;
  let err: string | null = $state(null);
  let exited = $state(false);

  const unlisteners: UnlistenFn[] = [];
  const enc = new TextEncoder();

  function toBase64(u8: Uint8Array): string {
    let bin = "";
    for (let i = 0; i < u8.length; i++) bin += String.fromCharCode(u8[i]);
    return btoa(bin);
  }
  function fromBase64(b64: string): Uint8Array {
    const bin = atob(b64);
    const u8 = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) u8[i] = bin.charCodeAt(i);
    return u8;
  }

  onMount(async () => {
    term = new Terminal({
      fontFamily: 'ui-monospace, Menlo, "JetBrains Mono", monospace',
      fontSize: 13,
      cursorBlink: true,
      allowProposedApi: true,
      theme: {
        background: "#0d1117",
        foreground: "#c9d1d9",
        cursor: "#c9d1d9",
        selectionBackground: "#264f78",
      },
      scrollback: 10000,
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    try {
      term.loadAddon(new WebglAddon());
    } catch {
      // WebGL unavailable; canvas fallback is fine.
    }
    fit.fit();

    try {
      const { sessionId: id } = await source.spawn(term.cols, term.rows);
      sessionId = id;
    } catch (e) {
      err = String(e);
      return;
    }

    const prefix = source.eventPrefix(sessionId);
    unlisteners.push(
      await listen<{ bytes_b64: string }>(`${prefix}:data`, (ev) => {
        term?.write(fromBase64(ev.payload.bytes_b64));
      }),
    );
    unlisteners.push(
      await listen<{ code: number | null }>(`${prefix}:exit`, () => {
        exited = true;
        term?.write("\r\n\x1b[2m[session ended]\x1b[0m\r\n");
        onExit?.();
      }),
    );

    term.onData((s) => {
      if (!sessionId || exited) return;
      source.write(sessionId, toBase64(enc.encode(s))).catch((e) => {
        err = String(e);
      });
    });
    term.onResize(({ cols, rows }) => {
      if (!sessionId) return;
      source.resize(sessionId, cols, rows).catch(() => {});
    });

    const ro = new ResizeObserver(() => fit?.fit());
    ro.observe(host);
    unlisteners.push(async () => ro.disconnect());

    term.focus();
  });

  onDestroy(async () => {
    for (const u of unlisteners) {
      try {
        await u();
      } catch {}
    }
    if (sessionId) {
      try {
        await source.close(sessionId);
      } catch {}
    }
    term?.dispose();
  });
</script>

<section class="pane">
  <div class="host" bind:this={host}></div>
  {#if err}<div class="err">{err}</div>{/if}
</section>

<style>
  .pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: #0d1117;
    min-width: 0;
    min-height: 0;
  }
  .host {
    flex: 1;
    padding: 8px;
    min-height: 0;
  }
  .err {
    background: #f8514922;
    color: #f85149;
    padding: 6px 10px;
    border-top: 1px solid #30363d;
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }
</style>
