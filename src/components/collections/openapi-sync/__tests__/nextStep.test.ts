import { describe, expect, it } from 'vitest';
import type { OpenApiSyncStatus } from '@/lib/tauri-api';
import { nextStep } from '../nextStep';

const GITHUB = 'https://github.com/acme/api/blob/main/openapi.yaml';
const OTHER = 'https://example.com/openapi.yaml';

function failed(code: string, location = GITHUB, consent = 'granted'): OpenApiSyncStatus {
  return {
    location,
    interval: 'off',
    lastCheckedAt: null,
    lastStatus: { state: 'failed', code },
    hasPending: false,
    consent,
  } as OpenApiSyncStatus;
}

describe('nextStep', () => {
  it('not_found_on_github_offers_a_token', () => {
    expect(nextStep(failed('not_found'))).toEqual({
      kind: 'add_token',
      label: 'Add access token',
    });
    expect(nextStep(failed('unauthorized')).kind).toBe('add_token');
  });

  it('no_token_action_for_a_non_github_link', () => {
    expect(nextStep(failed('not_found', OTHER)).kind).toBe('none');
    expect(nextStep(failed('unauthorized', OTHER)).kind).toBe('none');
  });

  it('network_offers_check_now', () => {
    for (const code of ['network', 'status', 'rate_limited', 'too_many_redirects']) {
      expect(nextStep(failed(code))).toEqual({ kind: 'check_now', label: 'Check now' });
    }
  });

  it('an_unknown_code_has_no_action', () => {
    expect(nextStep(failed('something_new')).kind).toBe('none');
    expect(nextStep({ ...failed('network'), lastStatus: null }).kind).toBe('none');
  });

  it('no_action_without_consent', () => {
    expect(nextStep(failed('network', GITHUB, 'missing')).kind).toBe('none');
    expect(nextStep(failed('not_found', GITHUB, 'locationChanged')).kind).toBe('none');
  });

  it('a_missing_file_offers_choose_another_file', () => {
    expect(nextStep(failed('file_missing', 'file:///a/api.yaml'))).toEqual({
      kind: 'change_link',
      label: 'Choose another file',
    });
    expect(nextStep(failed('file_missing', 'file:///a/api.yaml', 'missing')).kind).toBe('none');
  });
});
