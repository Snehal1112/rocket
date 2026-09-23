import { describe, expect, it } from 'vitest';
import { validateExternalSecretBindings } from './external-secrets';
import type { ExternalSecretBinding } from './tauri-api';

const binding = (overrides: Partial<ExternalSecretBinding> = {}): ExternalSecretBinding => ({
  alias: 'payments',
  connectionId: 'conn-1',
  vaultName: 'prod-vault',
  secretNames: [],
  ...overrides,
});

describe('validateExternalSecretBindings', () => {
  it('accepts valid bindings', () => {
    expect(
      validateExternalSecretBindings([binding(), binding({ alias: 'stripe_live-2' })]),
    ).toBeNull();
  });

  it.each([
    ['empty alias', [binding({ alias: '' })]],
    ['dotted alias', [binding({ alias: 'pay.ments' })]],
    ['duplicate alias', [binding(), binding()]],
    ['missing connection', [binding({ connectionId: '' })]],
    ['blank vault name', [binding({ vaultName: '  ' })]],
  ])('rejects %s', (_label, bindings) => {
    expect(validateExternalSecretBindings(bindings)).not.toBeNull();
  });
});
