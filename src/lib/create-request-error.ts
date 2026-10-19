// Turns a create-request failure into a sentence for a toast.
export function createRequestErrorMessage(err: unknown, name: string): string {
  const text = err instanceof Error ? err.message : String(err);
  if (/already exists/i.test(text)) {
    return `A request named "${name}" already exists here. Pick another name.`;
  }
  return `Could not create the request: ${text}`;
}
