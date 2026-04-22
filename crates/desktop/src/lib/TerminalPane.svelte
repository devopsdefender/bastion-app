<script>
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";

  let { connector } = $props();

  let mount = $state();
  let status = $state("opening…");
  let handle = null;
  let term = null;
  let fit = null;
  let unlisteners = [];

  async function open() {
    try {
      status = "attesting + opening /noise/shell/{id}…";
      const opened = await invoke("session_open", { connectorId: connector.id });
      handle = opened.handle;

      // Terminal
      term = new Terminal({
        fontFamily: "ui-monospace, Menlo, Consolas, monospace",
        fontSize: 13,
        theme: { background: "#11111b", foreground: "#cdd6f4" },
        cursorBlink: true,
        scrollback: 10000,
      });
      fit = new FitAddon();
      term.loadAddon(fit);
      term.open(mount);
      fit.fit();

      // Output pump: raw bytes → xterm.
      const un1 = await listen(`session:${handle}:output`, (ev) => {
        // Tauri deserializes Vec<u8> into a number[].
        const arr = ev.payload instanceof Uint8Array
          ? ev.payload
          : new Uint8Array(ev.payload);
        term.write(arr);
      });
      unlisteners.push(un1);

      // Control frames (block / exit / ready / error). For now just
      // show `exit` inline and close.
      const un2 = await listen(`session:${handle}:ctrl`, (ev) => {
        const msg = ev.payload;
        if (msg?.type === "exit") {
          term.writeln(`\r\n\x1b[2m[exited ${msg.code ?? "?"}]\x1b[0m`);
        } else if (msg?.type === "error") {
          term.writeln(`\r\n\x1b[31m[error: ${msg.code ?? "?"}]\x1b[0m`);
        }
      });
      unlisteners.push(un2);

      const un3 = await listen(`session:${handle}:close`, () => {
        status = "closed";
      });
      unlisteners.push(un3);

      // Initial resize + hello handshake with the remote replay
      // ring. have_up_to: -1 means "give me everything you've got."
      await invoke("session_resize", {
        handle,
        cols: term.cols,
        rows: term.rows,
      });
      await invoke("session_hello", { handle, haveUpTo: -1 });

      // stdin: xterm onData → session_input.
      term.onData((d) => {
        const bytes = new TextEncoder().encode(d);
        invoke("session_input", { handle, bytes: Array.from(bytes) });
      });

      // Resize: xterm onResize → session_resize.
      term.onResize(({ cols, rows }) => {
        invoke("session_resize", { handle, cols, rows });
      });

      // Refit on window resize (pane size changes too). FitAddon
      // computes the right cols/rows for the container.
      const onWinResize = () => {
        try {
          fit.fit();
        } catch {
          /* container not laid out yet */
        }
      };
      window.addEventListener("resize", onWinResize);
      unlisteners.push(() => window.removeEventListener("resize", onWinResize));

      status = "live";
      term.focus();
    } catch (e) {
      status = `error: ${e}`;
    }
  }

  async function close() {
    for (const u of unlisteners) {
      try {
        u();
      } catch {}
    }
    unlisteners = [];
    try {
      term?.dispose();
    } catch {}
    term = null;
    if (handle) {
      try {
        await invoke("session_close", { handle });
      } catch {}
      handle = null;
    }
  }

  // Re-open whenever the connector prop changes.
  $effect(() => {
    void connector.id; // read so the effect tracks it
    close().then(open);
  });

  onDestroy(() => {
    close();
  });
</script>

<div class="wrap">
  <header>
    <span class="label">{connector.label}</span>
    <span class="kind">{connector.kind}</span>
    <span class="status">{status}</span>
  </header>
  <div class="term" bind:this={mount}></div>
</div>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 12px;
    padding: 8px 12px;
    border-bottom: 1px solid #313244;
    background: #181825;
  }
  .label {
    font-weight: 600;
  }
  .kind {
    font-size: 11px;
    color: #6c7086;
  }
  .status {
    margin-left: auto;
    font-size: 12px;
    color: #6c7086;
  }
  .term {
    flex: 1;
    padding: 6px;
    min-height: 0;
    background: #11111b;
  }
</style>
