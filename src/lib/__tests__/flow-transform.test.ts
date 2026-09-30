import { describe, expect, it } from 'vitest';
import { DEFAULT_TRANSFORM_SCRIPT } from '../flow-transform';

describe('DEFAULT_TRANSFORM_SCRIPT', () => {
  it('matches TRANSFORM_DEFAULT_SCRIPT in rocket-flow', () => {
    expect(DEFAULT_TRANSFORM_SCRIPT).toBe('return response.body;');
  });
});
