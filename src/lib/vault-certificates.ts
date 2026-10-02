// src/lib/vault-certificates.ts

import { type as osType } from '@tauri-apps/plugin-os';
import type { VaultCertificateFormat, VaultCertificateSummary } from '@/lib/tauri-api';

// True for an elliptic-curve key, in any spelling RocketVault may use (EC, ECDSA, EC-P256).
export function isEcKeyAlgorithm(keyAlgorithm: string | undefined): boolean {
  return !!keyAlgorithm && /^ec/i.test(keyAlgorithm.trim());
}

// True when the app runs on Windows. Outside Tauri (tests, a browser) it is false.
export function isWindows(): boolean {
  try {
    return osType() === 'windows';
  } catch {
    return false;
  }
}

// The Windows TLS stack may not load an EC key from PEM, so the picker suggests PKCS12 there.
export function needsEcPemWarning(
  keyAlgorithm: string | undefined,
  format: VaultCertificateFormat,
  windows: boolean,
): boolean {
  return windows && format === 'pem' && isEcKeyAlgorithm(keyAlgorithm);
}

// A certificate can be picked only when RocketVault can export it.
export function isSelectable(summary: VaultCertificateSummary): boolean {
  return summary.exportable && summary.enabled;
}

// The picker label: the name, the key algorithm, and why it cannot be picked.
export function vaultCertificateLabel(summary: VaultCertificateSummary): string {
  let reason = '';
  if (!summary.exportable) reason = 'not exportable';
  else if (!summary.enabled) reason = 'disabled';
  return [summary.name, summary.keyAlgorithm, reason].filter(Boolean).join(' · ');
}
