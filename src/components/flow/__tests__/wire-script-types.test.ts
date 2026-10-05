import { describe, expect, it } from 'vitest';
import { WIRE_SCRIPT_TYPES } from '../wire-script-types';

describe('WIRE_SCRIPT_TYPES', () => {
  it('declares require() and every bundled module name', () => {
    expect(WIRE_SCRIPT_TYPES).toContain('declare function require');
    for (const name of ['lodash', 'uuid', 'moment', 'crypto-js', 'nanoid', 'chai']) {
      expect(WIRE_SCRIPT_TYPES).toContain(`'${name}'`);
    }
  });
});
