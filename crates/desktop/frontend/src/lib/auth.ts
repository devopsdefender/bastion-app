// Classify a thrown error as a "device isn't recognized" signal.
//
// Today the server closes the Noise_IK WebSocket without sending the
// second handshake message when the initiator's static pubkey isn't
// on the CP's allowlist. Surface shape:
//   "noise handshake: connection closed before second handshake message"
//
// We match loosely so minor server wording changes don't make the
// banner go silent. If we miss-classify a transient network close as
// an auth issue the worst case is a spurious Re-pair prompt — fine.

export function isAuthError(err: unknown): boolean {
  const s = String(err).toLowerCase();
  return (
    s.includes("connection closed before second handshake message") ||
    s.includes("unknown peer") ||
    s.includes("noise handshake")
  );
}
