// Shim so the Svelte language server doesn't complain about imports.
declare module "*.svelte" {
  import type { Component } from "svelte";
  const C: Component<any, any, any>;
  export default C;
}
