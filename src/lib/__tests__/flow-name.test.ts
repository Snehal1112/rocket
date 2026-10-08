import { describe, expect, it } from 'vitest';
import { validateFlowName } from '../flow-name';

describe('validateFlowName', () => {
  it('accepts an ordinary name', () => {
    expect(validateFlowName('Login then fetch')).toBeNull();
    expect(validateFlowName('  Sign In  ')).toBeNull();
  });

  it('rejects an empty name', () => {
    expect(validateFlowName('   ')).toBe('Enter a flow name.');
  });

  it('rejects "::" because Auth token keys are joined with it', () => {
    expect(validateFlowName('a::b')).toBe("A flow name cannot contain '::'.");
  });

  it('rejects the characters the sidebar rejects in names', () => {
    for (const bad of ['a/b', 'a\\b', 'a:b', 'a*b', 'a?b', 'a"b', 'a<b', 'a>b', 'a|b']) {
      expect(validateFlowName(bad)).not.toBeNull();
    }
  });

  it('rejects a name with no ASCII letter or digit, which the backend cannot store', () => {
    expect(validateFlowName('---')).toBe('A flow name needs at least one letter or digit.');
    expect(validateFlowName('üöï')).toBe('A flow name needs at least one letter or digit.');
  });
});
