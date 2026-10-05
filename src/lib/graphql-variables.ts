// Mirrors validate_variables in crates/rocket-app/src/graphql_request.rs, so the
// editor warns about the same text the backend would reject.
export function validateVariablesText(text: string): string | null {
  const trimmed = text.trim();
  // Text with a placeholder is not valid JSON until the placeholder is resolved.
  if (trimmed === '' || trimmed.includes('{{')) return null;
  try {
    const parsed: unknown = JSON.parse(trimmed);
    if (parsed === null || (typeof parsed === 'object' && !Array.isArray(parsed))) return null;
    return 'Variables must be a JSON object.';
  } catch (err) {
    return `Variables are not valid JSON: ${err instanceof Error ? err.message : String(err)}`;
  }
}
