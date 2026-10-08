import { describe, expect, it } from 'vitest';

// Every file that builds a scoped context must pass the secret key sets, or the
// popover and the hover show a secret environment value in clear text (F-57).
const sources = import.meta.glob<string>('/src/**/*.{ts,tsx}', {
  query: '?raw',
  import: 'default',
  eager: true,
});

const isTest = (path: string) => path.includes('/__tests__/') || /\.test\.tsx?$/.test(path);

const callers = Object.entries(sources).filter(
  ([path, text]) =>
    !isTest(path) && !path.endsWith('/lib/url-variables.ts') && text.includes('buildScopedContext('),
);

describe('buildScopedContext callers', () => {
  it('finds the known callers', () => {
    expect(callers.length).toBeGreaterThanOrEqual(8);
  });

  it.each(callers)('%s passes envSecretKeys', (_path, text) => {
    expect(text).toContain('envSecretKeys');
  });

  it.each(callers.filter(([, text]) => text.includes('globalVars')))(
    '%s passes globalSecretKeys',
    (_path, text) => {
      expect(text).toContain('globalSecretKeys');
    },
  );
});
