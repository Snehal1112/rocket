// Mirrors validate_variables in crates/rocket-app/src/graphql_request.rs, so the
// editor warns about the same text the backend would reject.
export function validateVariablesText(text: string): string | null {
  const trimmed = text.trim();
  if (trimmed === '') return null;
  // A placeholder stands for a value that is only known once it is resolved, so check the
  // rest of the text with each placeholder read as null.
  const masked = trimmed.replace(/\{\{[^{}]*\}\}/g, 'null');
  try {
    const parsed: unknown = JSON.parse(masked);
    if (parsed === null || (typeof parsed === 'object' && !Array.isArray(parsed))) return null;
    return 'Variables must be a JSON object.';
  } catch (err) {
    return `Variables are not valid JSON: ${err instanceof Error ? err.message : String(err)}`;
  }
}
