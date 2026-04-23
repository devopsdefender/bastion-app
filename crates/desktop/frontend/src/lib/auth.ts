// Error classifiers for connector-scoped issues we want to surface
// as banners.
//
// Three distinct failure modes bastion can detect today, each with a
// different remediation path:
//
// 1. `isAuthError` — the Noise_IK handshake closes before the second
//    message. Usually means the device pubkey isn't on the CP's
//    allowlist; the fix is a re-pair flow in bastion.
//
// 2. `isAttestBlocked` — `/attest` returned a 3xx, which typically
//    means Cloudflare Access is intercepting a route that's supposed
//    to be public. Server-side policy fix on the `dd` repo.
//
// 3. `isTmuxMissing` — the agent doesn't have tmux installed.
//    Bastion's session model depends on tmux; admin fix on the agent.

export function isAuthError(err: unknown): boolean {
  const s = String(err).toLowerCase();
  return (
    s.includes("connection closed before second handshake message") ||
    s.includes("unknown peer") ||
    s.includes("noise handshake")
  );
}

export function isAttestBlocked(err: unknown): boolean {
  const s = String(err).toLowerCase();
  return (
    s.includes("/attest is not publicly reachable") ||
    s.includes("cloudflare access") ||
    /get .* -> 30\d/.test(s)
  );
}

export function isTmuxMissing(err: unknown): boolean {
  const s = String(err).toLowerCase();
  return (
    s.includes("tmux is not installed on agent") ||
    s.includes("tmux: command not found") ||
    s.includes("command not found: tmux")
  );
}

export type ConnectorIssue =
  | { kind: "reauth"; connector_id: string }
  | {
      kind: "attest_blocked";
      connector_id: string;
      agent_origin: string;
    }
  | {
      kind: "tmux_missing";
      connector_id: string;
      agent_origin: string;
      agent_label: string;
    };
