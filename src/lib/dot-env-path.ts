// src/lib/dot-env-path.ts

/** Turns a picked .env path into the value to store: relative inside the base, else as picked. */
export function toStoredDotEnvPath(picked: string, baseDir: string | null): string {
  if (!baseDir) return picked;
  const normalPicked = picked.replace(/\\/g, '/');
  // A path with a parent segment is never stored in the relative form.
  if (normalPicked.split('/').includes('..')) return picked;
  const normalBase = baseDir.replace(/\\/g, '/').replace(/\/+$/, '');
  if (!normalBase) return picked;
  // The separator in the prefix keeps a sibling like /col2 outside of /col.
  const prefix = `${normalBase}/`;
  if (normalPicked.startsWith(prefix) && normalPicked.length > prefix.length) {
    return normalPicked.slice(prefix.length);
  }
  return picked;
}
