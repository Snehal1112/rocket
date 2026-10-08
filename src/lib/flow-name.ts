// The characters the collections sidebar rejects in collection names.
const INVALID_CHARS = /[/\\:*?"<>|]/;

/**
 * Returns why `raw` is not a usable flow name, or null when it is fine.
 * "::" is rejected explicitly because Auth token keys join their parts with
 * it and `flowAuthKeyMatches` matches by prefix. The letter or digit rule
 * mirrors the backend, which derives the file name from ASCII letters and digits.
 */
export function validateFlowName(raw: string): string | null {
  const name = raw.trim();
  if (!name) return 'Enter a flow name.';
  if (name.includes('::')) return "A flow name cannot contain '::'.";
  if (INVALID_CHARS.test(name)) return 'A flow name cannot contain / \\ : * ? " < > |.';
  if (!/[A-Za-z0-9]/.test(name)) return 'A flow name needs at least one letter or digit.';
  return null;
}
