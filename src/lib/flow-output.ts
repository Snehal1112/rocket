/** Pretty-prints a JSON object or array value; any other value is returned unchanged. */
export function formatOutputValue(value: string): string {
  try {
    const parsed: unknown = JSON.parse(value);
    if (typeof parsed === 'object' && parsed !== null) {
      return JSON.stringify(parsed, null, 2);
    }
  } catch {
    // Not JSON, so show the value as is.
  }
  return value;
}
