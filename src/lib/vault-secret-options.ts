// src/lib/vault-secret-options.ts

import type { ExternalSecretBinding } from '@/lib/tauri-api';

export interface VaultSecretOption {
  // The reference stored in the environment file: `${alias}.${secretName}`.
  value: string;
  label: string;
}

// Lists the secrets an environment can reference, from its External Secrets bindings.
// Only fetched secret names appear, never values.
export function vaultSecretOptions(bindings: ExternalSecretBinding[]): VaultSecretOption[] {
  const options: VaultSecretOption[] = [];
  for (const binding of bindings) {
    if (!binding.alias) continue;
    for (const ref of binding.secretNames) {
      const value = `${binding.alias}.${ref.name}`;
      options.push({ value, label: value });
    }
  }
  return options;
}
