import { describe, expect, it } from 'vitest';
import { GENERIC_SYNC_ERROR, syncErrorMessage } from '../syncErrors';

describe('syncErrorMessage', () => {
  it('maps the consent_required prefix', () => {
    const text = syncErrorMessage(
      'Conflict: consent_required: approve this spec link on this machine first.',
    );
    expect(text).toMatch(/approve/i);
    expect(text).not.toContain('consent_required');
  });

  it('maps the disconnect busy sentence', () => {
    expect(
      syncErrorMessage('An apply is running on this collection. Try again when it finishes.'),
    ).toBe('An apply is running on this collection. Try again when it finishes.');
  });

  it('maps the location_changed prefix', () => {
    const text = syncErrorMessage(
      'Conflict: location_changed: the spec link changed, approve the new link first.',
    );
    expect(text).toMatch(/changed/i);
    expect(text).not.toContain('location_changed');
  });

  it('maps the two grant sentences', () => {
    expect(
      syncErrorMessage('Conflict: The spec link changed. Check the link and approve it again.'),
    ).toMatch(/link changed/i);
    expect(
      syncErrorMessage(
        'Invalid input: This spec location cannot be approved because its link is ambiguous.',
      ),
    ).toMatch(/cannot be approved/i);
  });

  it('maps the pending spec changed conflict', () => {
    expect(syncErrorMessage('Conflict: the pending spec changed, check again')).toMatch(
      /check again/i,
    );
  });

  it('maps the busy and another apply wording', () => {
    expect(syncErrorMessage('Conflict: an earlier apply is not finished yet')).toMatch(
      /another apply|still running|not finished/i,
    );
  });

  it('accepts an Error instance', () => {
    expect(syncErrorMessage(new Error('Conflict: the pending spec changed, check again'))).toMatch(
      /check again/i,
    );
  });

  it('returns a generic sentence for anything else, without a path', () => {
    const raw = 'IO error: cannot read /home/me/secret/.rocket/openapi-source.yml';
    const text = syncErrorMessage(raw);
    expect(text).toBe(GENERIC_SYNC_ERROR);
    expect(text).not.toContain('/home');
    expect(syncErrorMessage(undefined)).toBe(GENERIC_SYNC_ERROR);
    expect(syncErrorMessage({ a: 1 })).toBe(GENERIC_SYNC_ERROR);
  });

  it('maps the disconnect failure sentence', () => {
    expect(syncErrorMessage('The sync settings could not be removed. Try again.')).toBe(
      'The sync settings could not be removed. Try again.',
    );
  });
});
