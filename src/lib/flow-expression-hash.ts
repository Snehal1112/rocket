const FNV_OFFSET = 0x811c9dc5;
const FNV_PRIME = 0x01000193;

// FNV-1a 32-bit over the UTF-8 bytes of the text, as eight lowercase hex digits.
// The backend has the same function (`expression_hash` in flow_trace.rs).
export function expressionHash(text: string): string {
  let hash = FNV_OFFSET;
  for (const byte of new TextEncoder().encode(text)) {
    hash ^= byte;
    hash = Math.imul(hash, FNV_PRIME);
  }
  return (hash >>> 0).toString(16).padStart(8, '0');
}
