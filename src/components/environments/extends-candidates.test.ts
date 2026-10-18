// src/components/environments/extends-candidates.test.ts

import { describe, expect, it } from 'vitest';
import type { Environment } from '@/lib/tauri-api';
import { extendsCandidates } from './extends-candidates';

function env(name: string, ext?: string): Environment {
  return { name, variables: [], extends: ext };
}

describe('extendsCandidates', () => {
  it('extends_candidates_exclude_the_environment_itself', () => {
    expect(extendsCandidates([env('a'), env('b')], 'a')).toEqual(['b']);
  });

  it('extends_candidates_exclude_a_descendant_that_would_create_a_cycle', () => {
    const envs = [env('base'), env('mid', 'base'), env('leaf', 'mid'), env('other')];
    expect(extendsCandidates(envs, 'base')).toEqual(['other']);
    expect(extendsCandidates(envs, 'mid')).toEqual(['base', 'other']);
  });

  it('extends_candidates_keep_unrelated_environments_sorted', () => {
    const envs = [env('zeta'), env('alpha'), env('me'), env('mid')];
    expect(extendsCandidates(envs, 'me')).toEqual(['alpha', 'mid', 'zeta']);
  });

  it('terminates on an existing cycle in stored data', () => {
    const envs = [env('x', 'y'), env('y', 'x'), env('me')];
    expect(extendsCandidates(envs, 'me')).toEqual(['x', 'y']);
  });
});
