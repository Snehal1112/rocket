// src/lib/certificate-validation.ts

import type { ClientCertificate, ExternalSecretBinding } from '@/lib/tauri-api';

type FileCertificate = Exclude<ClientCertificate, { type: 'vault' }>;
type VaultCertificate = Extract<ClientCertificate, { type: 'vault' }>;

const VAULT_FORMATS: readonly string[] = ['pem', 'pkcs12'];

export interface CertificateIssues {
  errors: string[];
  warnings: string[];
}

const KEY_TEXT_PREFIX = '-----BEGIN';
const PLACEHOLDER = /\{\{[^}]+\}\}/;

// True for a passphrase that would be written to the file as typed.
export function isLiteralPassphrase(passphrase: string | undefined): boolean {
  return !!passphrase && !PLACEHOLDER.test(passphrase);
}

interface Piece {
  label: string;
  filePath: string;
  secret: string;
}

function piecesOf(cert: FileCertificate): Piece[] {
  if (cert.type === 'pem') {
    return [
      {
        label: 'Certificate',
        filePath: cert.certificateFilePath ?? '',
        secret: cert.certificateSecret ?? '',
      },
      {
        label: 'Private key',
        filePath: cert.privateKeyFilePath ?? '',
        secret: cert.privateKeySecret ?? '',
      },
    ];
  }
  return [
    {
      label: 'PKCS12 bundle',
      filePath: cert.pkcs12FilePath ?? '',
      secret: cert.pkcs12Secret ?? '',
    },
  ];
}

function isKeyText(value: string): boolean {
  return value.trimStart().startsWith(KEY_TEXT_PREFIX);
}

// Mirrors the `vault` rules of `validate_client_certificates` in rocket-environment.
function vaultCertificateErrors(
  who: string,
  cert: VaultCertificate,
  bindings: ExternalSecretBinding[],
): string[] {
  const named: [string, string][] = [
    ['domain', cert.domain],
    ['binding', cert.binding],
    ['certificate', cert.certificate],
  ];
  const keyText = named
    .filter(([, value]) => isKeyText(value))
    .map(([field]) => `${who}: ${field} must be a name, not key text.`);
  if (keyText.length > 0) return keyText;

  const errors: string[] = [];
  if (cert.binding.trim() === '') {
    errors.push(`${who}: choose an External Secrets binding.`);
  } else if (!bindings.some((b) => b.alias === cert.binding)) {
    errors.push(
      `${who}: binding "${cert.binding}" has no External Secrets binding in this environment.`,
    );
  }
  if (cert.certificate.trim() === '') errors.push(`${who}: choose a certificate.`);
  if (cert.format !== undefined && !VAULT_FORMATS.includes(cert.format)) {
    errors.push(`${who}: format must be PEM or PKCS12.`);
  }
  return errors;
}

// Absolute, home and variable-prefixed paths are not relative. A variable
// prefix is unknown until it resolves, so it is not checked here.
function isRelativePath(path: string): boolean {
  if (path.startsWith('{{')) return false;
  if (path.startsWith('/') || path.startsWith('\\')) return false;
  if (path === '~' || path.startsWith('~/')) return false;
  return !/^[A-Za-z]:[\\/]/.test(path);
}

function hasParentSegment(path: string): boolean {
  return path.split(/[\\/]/).includes('..');
}

// Returns why a reference is unusable, or null when it matches a binding and one of its names.
function referenceProblem(reference: string, bindings: ExternalSecretBinding[]): string | null {
  const dot = reference.indexOf('.');
  if (dot <= 0 || dot === reference.length - 1) return 'must look like alias.secretName';
  const alias = reference.slice(0, dot);
  const name = reference.slice(dot + 1);
  const binding = bindings.find((b) => b.alias === alias);
  if (!binding) return `has no External Secrets binding with alias "${alias}"`;
  if (!binding.secretNames.some((ref) => ref.name === name)) {
    return `is not one of the fetched secrets for alias "${alias}". Fetch the secrets on the External Secrets tab`;
  }
  return null;
}

// Mirrors `validate_client_certificates` in rocket-environment. Errors block the save.
// A literal passphrase is only a warning.
export function validateClientCertificates(
  certs: ClientCertificate[],
  bindings: ExternalSecretBinding[],
): CertificateIssues {
  const errors: string[] = [];
  const warnings: string[] = [];

  for (const [idx, cert] of certs.entries()) {
    const domain = cert.domain.trim();
    // A pasted key in the domain must not be echoed back in every message.
    const who =
      domain && !isKeyText(domain)
        ? `Certificate ${idx + 1} (${domain})`
        : `Certificate ${idx + 1}`;
    if (!domain) errors.push(`${who}: domain is required.`);

    if (cert.type === 'vault') {
      errors.push(...vaultCertificateErrors(who, cert, bindings));
      continue;
    }

    for (const piece of piecesOf(cert)) {
      const hasFile = piece.filePath.trim() !== '';
      const hasSecret = piece.secret.trim() !== '';
      if (hasFile && hasSecret) {
        errors.push(
          `${who}: ${piece.label} must have either a file path or a vault secret, not both.`,
        );
        continue;
      }
      if (!hasFile && !hasSecret) {
        errors.push(`${who}: ${piece.label} needs a file path or a vault secret.`);
        continue;
      }
      if (hasFile) {
        if (isKeyText(piece.filePath)) {
          errors.push(
            `${who}: ${piece.label} file path must be a file path or a vault secret reference, not key text.`,
          );
        } else if (isRelativePath(piece.filePath) && hasParentSegment(piece.filePath)) {
          errors.push(`${who}: ${piece.label} file path must not contain "..".`);
        }
      } else if (isKeyText(piece.secret)) {
        errors.push(
          `${who}: ${piece.label} secret must be a file path or a vault secret reference, not key text.`,
        );
      } else {
        const problem = referenceProblem(piece.secret, bindings);
        if (problem) errors.push(`${who}: ${piece.label} secret "${piece.secret}" ${problem}.`);
      }
    }

    if (isLiteralPassphrase(cert.passphrase)) {
      warnings.push(
        `${who}: the passphrase is a literal and will be saved in the environment file. Use a vault secret placeholder such as {{vault.NAME}}.`,
      );
    }
  }

  return { errors, warnings };
}
