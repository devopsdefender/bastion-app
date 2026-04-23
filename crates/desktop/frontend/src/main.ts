import { mount } from "svelte";
import App from "./App.svelte";

// Dev-only: mirror webview console errors/warnings and uncaught
// exceptions into the Tauri Rust stdout via the `dev_log` command.
// Lets the terminal running `tauri dev` see browser-side failures
// without opening devtools.
if (import.meta.env.DEV) {
  import("@tauri-apps/api/core").then(({ invoke }) => {
    function fmt(a: unknown): string {
      if (typeof a === "string") return a;
      if (a instanceof Error) return a.stack || a.message;
      try {
        return JSON.stringify(a);
      } catch {
        return String(a);
      }
    }
    function ship(level: string, args: unknown[]) {
      const msg = args.map(fmt).join(" ");
      invoke("dev_log", { args: { level, msg } }).catch(() => {});
    }
    const origError = console.error.bind(console);
    const origWarn = console.warn.bind(console);
    console.error = (...args: unknown[]) => {
      ship("error", args);
      origError(...args);
    };
    console.warn = (...args: unknown[]) => {
      ship("warn", args);
      origWarn(...args);
    };
    window.addEventListener("error", (e) => {
      ship("error", [
        `uncaught: ${e.message} @ ${e.filename}:${e.lineno}:${e.colno}`,
        e.error,
      ]);
    });
    window.addEventListener("unhandledrejection", (e) => {
      ship("error", [`unhandled rejection:`, e.reason]);
    });
  });
}

const target = document.getElementById("app");
if (!target) throw new Error("missing #app");
mount(App, { target });
