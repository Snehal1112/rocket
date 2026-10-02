// src/lib/certificate-validation.ts

import type { ClientCertificate, ExternalSecretBinding } from '@/lib/tauri-api';

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

function piecesOf(cert: ClientCertificate): Piece[] {
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
    const who = domain ? `Certificate ${idx + 1} (${domain})` : `Certificate ${idx + 1}`;
    if (!domain) errors.push(`${who}: domain is required.`);

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
