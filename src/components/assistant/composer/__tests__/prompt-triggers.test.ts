import { describe, expect, it } from 'vitest';
import { matchTrigger } from '../prompt-triggers';

describe('matchTrigger', () => {
  it('matches a bare # at the start of the prompt', () => {
    expect(matchTrigger('#', 0)).toEqual({ kind: 'reference', from: 0, query: '' });
  });

  it('matches a # after a space and reports where it starts', () => {
    expect(matchTrigger('see #ord', 0)).toEqual({ kind: 'reference', from: 4, query: 'ord' });
  });

  it('adds the line offset on a later line', () => {
    expect(matchTrigger('#env', 6)).toEqual({ kind: 'reference', from: 6, query: 'env' });
  });

  it('ignores a # inside a word', () => {
    expect(matchTrigger('issue#12', 0)).toBeNull();
  });

  it('ignores a finished reference followed by a space', () => {
    expect(matchTrigger('#orders now', 0)).toBeNull();
  });

  it('matches a / command at the very start of the prompt', () => {
    expect(matchTrigger('/ex', 0)).toEqual({ kind: 'command', from: 0, query: 'ex' });
  });

  it('ignores a / command on a later line', () => {
    expect(matchTrigger('/ex', 5)).toBeNull();
  });

  it('ignores a / after other text', () => {
    expect(matchTrigger('say /ex', 0)).toBeNull();
  });

  it('ignores a / command once a space follows it', () => {
    expect(matchTrigger('/tests now', 0)).toBeNull();
  });
});
