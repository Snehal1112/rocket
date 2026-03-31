/**
 * Each unsafe character (individually). Covers: / \ : * ? " < > | [ ]
 */
const UNSAFE_RE = /[/\\:*?"<>|[\]]/g;

/**
 * Runs of the same repeated unsafe character collapse to a single `-`.
 * This regex matches two or more consecutive identical dash-replacement chars,
 * but we achieve it by collapsing same-character runs before replacement.
 */
const SAME_CHAR_RUN_RE = /([/\\:*?"<>|[\]])\1+/g;

/**
 * Converts a display name into a filesystem-safe `.yml` filename.
 *
 * Rules:
 * - Leading/trailing whitespace is stripped.
 * - Runs of identical unsafe characters collapse to a single replacement `-`.
 * - Each remaining unsafe character is replaced with `-`.
 * - Falls back to "request" if the result has no non-dash content.
 * - Always appends `.yml`.
 */
export function sanitizeFilename(displayName: string): string {
  const sanitized = displayName
    .trim()
    .replace(SAME_CHAR_RUN_RE, '$1') // collapse identical runs first
    .replace(UNSAFE_RE, '-')
    .trim();

  // Fall back when the result is empty or contains only dashes.
  const hasContent = sanitized.replace(/-/g, '').length > 0;
  return `${hasContent ? sanitized : 'request'}.yml`;
}
