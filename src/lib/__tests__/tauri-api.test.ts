import { describe, expect, it } from 'vitest';
import { isGitSshTrustFailure, parseGitNetworkError } from '../tauri-api';

describe('parseGitNetworkError', () => {
  it('parses an SSH unknown-host failure', () => {
    const parsed = parseGitNetworkError({
      code: 'sshUnknownHost',
      message:
        'Unknown SSH host git.example.com:22 (algorithm ssh-ed25519, fingerprint SHA256:abc)',
      host: 'git.example.com',
      port: 22,
      algorithm: 'ssh-ed25519',
      fingerprint: 'SHA256:abc',
    });

    expect(parsed.code).toBe('sshUnknownHost');
    expect(isGitSshTrustFailure(parsed)).toBe(true);
  });

  it('parses a TLS certificate failure without an algorithm field', () => {
    const parsed = parseGitNetworkError({
      code: 'tlsCertificateInvalid',
      message: 'Invalid TLS certificate for git.example.com:443 (fingerprint SHA256:invalid)',
      host: 'git.example.com',
      port: 443,
      fingerprint: 'SHA256:invalid',
    });

    expect(parsed).toEqual({
      code: 'tlsCertificateInvalid',
      message: 'Invalid TLS certificate for git.example.com:443 (fingerprint SHA256:invalid)',
      host: 'git.example.com',
      port: 443,
      fingerprint: 'SHA256:invalid',
    });
    expect(isGitSshTrustFailure(parsed)).toBe(false);
  });

  it('falls back to generic for an unrecognized shape', () => {
    expect(parseGitNetworkError(new Error('boom'))).toEqual({
      code: 'generic',
      message: 'Error: boom',
    });
  });

  it('falls back to generic when a TLS failure is missing required fields', () => {
    expect(
      parseGitNetworkError({
        code: 'tlsCertificateInvalid',
        message: 'Invalid TLS certificate',
        host: 'git.example.com',
        // port missing
      }),
    ).toEqual({
      code: 'generic',
      message: 'Invalid TLS certificate',
    });
  });
});
