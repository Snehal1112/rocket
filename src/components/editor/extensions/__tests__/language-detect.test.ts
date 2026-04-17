import { describe, expect, it } from 'vitest';
import {
  detectLanguage,
  getLanguageExtension,
  getLanguageExtensionForFile,
} from '../language-detect';

describe('detectLanguage', () => {
  it('detects json from bodyMode', () => {
    expect(detectLanguage('json')).toBe('json');
  });

  it('detects xml from bodyMode', () => {
    expect(detectLanguage('xml')).toBe('xml');
  });

  it('detects plaintext from text bodyMode', () => {
    expect(detectLanguage('text')).toBe('plaintext');
  });

  it('detects json from Content-Type header', () => {
    expect(detectLanguage(undefined, 'application/json; charset=utf-8')).toBe('json');
  });

  it('detects html from Content-Type header', () => {
    expect(detectLanguage(undefined, 'text/html')).toBe('html');
  });

  it('detects yaml from Content-Type header', () => {
    expect(detectLanguage(undefined, 'application/x-yaml')).toBe('yaml');
  });

  it('returns plaintext for unknown Content-Type', () => {
    expect(detectLanguage(undefined, 'application/octet-stream')).toBe('plaintext');
  });

  it('bodyMode takes precedence over contentType', () => {
    expect(detectLanguage('json', 'text/html')).toBe('json');
  });
});

describe('getLanguageExtension', () => {
  it('returns an extension for json', () => {
    expect(getLanguageExtension('json')).not.toBeNull();
  });

  it('returns null for plaintext', () => {
    expect(getLanguageExtension('plaintext')).toBeNull();
  });

  it('returns null for unknown language', () => {
    expect(getLanguageExtension('fortran')).toBeNull();
  });
});

describe('getLanguageExtensionForFile', () => {
  it('returns yaml for .yml files', () => {
    expect(getLanguageExtensionForFile('auth/login.yml')).not.toBeNull();
  });

  it('returns json for .json files', () => {
    expect(getLanguageExtensionForFile('config.json')).not.toBeNull();
  });

  it('returns typescript for .ts files', () => {
    expect(getLanguageExtensionForFile('utils.ts')).not.toBeNull();
  });

  it('returns null for .bru files', () => {
    expect(getLanguageExtensionForFile('request.bru')).toBeNull();
  });

  it('returns null for extensionless files', () => {
    expect(getLanguageExtensionForFile('Dockerfile')).toBeNull();
  });
});
