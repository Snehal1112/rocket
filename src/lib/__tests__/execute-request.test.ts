import { describe, expect, it } from 'vitest';
import { getEnvInvalidationKeys } from '@/lib/execute-request';
import { environmentKeys } from '@/lib/queries/environment-queries';

describe('getEnvInvalidationKeys', () => {
  it('returns the collection environments key when collection is set', () => {
    const keys = getEnvInvalidationKeys('my-api', undefined);
    expect(keys).toContainEqual(environmentKeys.collection('my-api'));
  });

  it('returns the global environment key when globalEnvName is set', () => {
    const keys = getEnvInvalidationKeys(undefined, 'global-prod');
    expect(keys).toContainEqual(environmentKeys.global('global-prod'));
  });

  it('returns both keys when both are set', () => {
    const keys = getEnvInvalidationKeys('my-api', 'global-prod');
    expect(keys).toContainEqual(environmentKeys.collection('my-api'));
    expect(keys).toContainEqual(environmentKeys.global('global-prod'));
  });

  it('returns an empty list when neither is set', () => {
    const keys = getEnvInvalidationKeys(undefined, undefined);
    expect(keys).toHaveLength(0);
  });
});
