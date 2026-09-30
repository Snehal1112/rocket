// Mirrors is_sensitive_header and REDACTED in crates/rocket-app/src/redaction.rs.
// Keep both lists in step.
const SENSITIVE = new Set([
  'authorization',
  'proxy-authorization',
  'cookie',
  'set-cookie',
  'x-api-key',
]);

/** The marker the backend shows in place of a masked value. */
export const REDACTED_VALUE = '••••••';

/** True for a header whose value must never be shown. */
export function isSensitiveHeader(name: string): boolean {
  return SENSITIVE.has(name.toLowerCase());
}
