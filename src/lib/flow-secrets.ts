import type { Auth } from '@/lib/tauri-api';

// One or more `{{variable}}` references and nothing else, such as `{{token}}`.
const ONLY_REFERENCES = /^\s*(?:\{\{[^{}]+\}\}\s*)+$/;

/** True when the value is only `{{variable}}` references, so it holds no literal secret. */
export function isVariableReference(value: string): boolean {
  return ONLY_REFERENCES.test(value);
}

type Path = readonly string[];

interface SecretField {
  label: string;
  path: Path;
  // Narrows a field to some shapes of the auth, such as an inline OAuth 1.0 key.
  when?: (auth: Auth) => boolean;
}

const PASSWORD: readonly SecretField[] = [{ label: 'Password', path: ['password'] }];

// The credential fields of each persisted auth type, in the order they are listed.
// The names are the flat persisted names, not the editor's nested `AuthState`.
const SECRET_FIELDS: Record<string, readonly SecretField[]> = {
  bearer: [{ label: 'Token', path: ['token'] }],
  basic: PASSWORD,
  digest: PASSWORD,
  wsse: PASSWORD,
  ntlm: PASSWORD,
  'api-key': [{ label: 'API key value', path: ['value'] }],
  'o-auth2': [
    { label: 'Client secret', path: ['credentials', 'clientSecret'] },
    { label: 'Resource owner password', path: ['resourceOwner', 'password'] },
  ],
  'aws-sig-v4': [
    { label: 'Secret access key', path: ['secretKey'] },
    { label: 'Session token', path: ['sessionToken'] },
  ],
  'o-auth1': [
    { label: 'Consumer secret', path: ['consumerSecret'] },
    { label: 'Access token', path: ['accessToken'] },
    { label: 'Access token secret', path: ['accessTokenSecret'] },
    {
      label: 'Private key',
      path: ['privateKey', 'value'],
      when: (auth) => readString(auth, ['privateKey', 'type']) === 'text',
    },
  ],
};

function readString(root: unknown, path: Path): string | undefined {
  let current: unknown = root;
  for (const part of path) {
    if (typeof current !== 'object' || current === null) return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return typeof current === 'string' ? current : undefined;
}

function isPlaintext(value: string | undefined): boolean {
  return value !== undefined && value.trim() !== '' && !isVariableReference(value);
}

// The fields of `auth` that hold a literal credential right now.
function plaintextFields(auth: Auth): SecretField[] {
  return (SECRET_FIELDS[auth.authType] ?? []).filter(
    (field) => (!field.when || field.when(auth)) && isPlaintext(readString(auth, field.path)),
  );
}

// A copy of `root` with the string at `path` replaced. Missing parents are not created.
function withString(
  root: Record<string, unknown>,
  path: Path,
  value: string,
): Record<string, unknown> {
  const [head, ...rest] = path;
  if (rest.length === 0) return { ...root, [head]: value };
  const child = root[head];
  if (typeof child !== 'object' || child === null) return root;
  return { ...root, [head]: withString(child as Record<string, unknown>, rest, value) };
}

/**
 * Labels of the credential fields in a persisted auth that hold literal text.
 * A field that is empty or only a `{{variable}}` reference is not listed.
 * Returns labels only, never values. An OAuth 2.0 token held in memory is not
 * part of the persisted auth and is never listed.
 */
export function plaintextSecretFields(auth: Auth): string[] {
  return plaintextFields(auth).map((field) => field.label);
}

/**
 * A copy of `auth` with every literal credential replaced by `replacement`, and
 * the labels of the fields that were replaced. `auth` itself is not changed.
 */
export function redactPlaintextSecrets(
  auth: Auth,
  replacement: string,
): { auth: Auth; fields: string[] } {
  const found = plaintextFields(auth);
  let next = auth as unknown as Record<string, unknown>;
  for (const field of found) next = withString(next, field.path, replacement);
  return { auth: next as unknown as Auth, fields: found.map((field) => field.label) };
}
