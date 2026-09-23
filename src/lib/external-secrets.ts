import type { ExternalSecretBinding } from '@/lib/tauri-api';

// Letters, digits, '_' and '-' only. A dot would make `{{alias.secretName}}` ambiguous.
const ALIAS_PATTERN = /^[A-Za-z0-9_-]+$/;

// Mirrors `validate_external_secret_bindings` in rocket-environment.
// Returns a user-facing message for the first invalid binding, or null when all are valid.
export function validateExternalSecretBindings(bindings: ExternalSecretBinding[]): string | null {
  const seen = new Set<string>();
  for (const [idx, binding] of bindings.entries()) {
    const label = binding.alias || `#${idx + 1}`;
    if (!binding.alias) return `External secret binding ${label} needs an alias.`;
    if (!ALIAS_PATTERN.test(binding.alias)) {
      return `Alias "${binding.alias}" may only contain letters, digits, "_" or "-".`;
    }
    if (seen.has(binding.alias)) return `Alias "${binding.alias}" is used more than once.`;
    seen.add(binding.alias);
    if (!binding.connectionId) return `External secret binding "${label}" needs a connection.`;
    if (!binding.vaultName.trim()) return `External secret binding "${label}" needs a vault name.`;
  }
  return null;
}
