// The standard HTTP methods offered in the method selector, in display order.
export const STANDARD_METHODS: readonly string[] = [
  'GET',
  'POST',
  'PUT',
  'PATCH',
  'DELETE',
  'OPTIONS',
  'HEAD',
  'TRACE',
  'CONNECT',
];

// RFC 9110 token characters. Mirrors `is_valid_method_token` in rocket-shared.
const METHOD_TOKEN = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]{1,64}$/;

export function isValidMethodToken(value: string): boolean {
  return METHOD_TOKEN.test(value);
}

// Turns typed text into the method to store. Standard names are upper-cased, any other
// valid token is kept exactly as typed (methods are case-sensitive). Returns null for
// text that is not a method token.
export function normalizeMethod(input: string): string | null {
  const trimmed = input.trim();
  if (!isValidMethodToken(trimmed)) return null;
  const upper = trimmed.toUpperCase();
  return STANDARD_METHODS.includes(upper) ? upper : trimmed;
}

// Keeps the current method visible in the selector even when it is a custom one.
export function withCurrentMethod(options: readonly string[], current: string): string[] {
  return options.includes(current) ? (options as string[]) : [...options, current];
}
