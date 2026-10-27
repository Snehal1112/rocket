import { describe, expect, it } from 'vitest';
import {
  changeLinkWarningMessage,
  connectWarningMessage,
  GENERIC_SYNC_ERROR,
  isStaleConnect,
  syncErrorMessage,
} from '../syncErrors';

describe('syncErrorMessage', () => {
  it('maps the consent_required prefix', () => {
    const text = syncErrorMessage(
      'Conflict: consent_required: approve this spec link on this machine first.',
    );
    expect(text).toMatch(/approve/i);
    expect(text).not.toContain('consent_required');
  });

  it('maps_the_unshowable_spec_sentence', () => {
    expect(syncErrorMessage('Invalid input: The saved spec cannot be shown.')).toBe(
      'The saved spec cannot be shown.',
    );
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

  it('maps_the_connect_sentences_and_spots_a_stale_preview', () => {
    expect(syncErrorMessage('The collection changed. Run the preview again.')).toBe(
      'The collection changed. Run the preview again.',
    );
    expect(isStaleConnect('The spec changed. Run the preview again.')).toBe(true);
    expect(isStaleConnect('Connecting failed. Nothing was changed.')).toBe(false);
    expect(syncErrorMessage('Connecting failed. Nothing was changed. /home/u/x')).toBe(
      'Connecting failed. Nothing was changed.',
    );
  });

  it('lets_only_the_two_fixed_connect_warnings_through', () => {
    expect(connectWarningMessage('Checks are not enabled yet.')).toBe('Checks are not enabled yet.');
    expect(connectWarningMessage('The token was not saved.')).toBe('The token was not saved.');
    expect(connectWarningMessage('keychain said no at /home/u')).toBeNull();
  });

  it('keeps_the_fixed_fetch_sentences_that_tell_the_user_what_to_do', () => {
    for (const sentence of [
      'The file was not found, or it needs a sign-in. For a private GitHub repository, add an access token.',
      'The file was not found, or the token was rejected or lacks Contents read access.',
      'The file is larger than the 25 MB limit.',
      'Tokens work with GitHub links only.',
    ]) {
      expect(syncErrorMessage(sentence)).toBe(sentence);
    }
  });

  it('maps_the_missing_pending_copy_sentence', () => {
    const text = 'A newer spec is waiting but its copy is missing. Run Check now.';
    expect(syncErrorMessage(`Invalid input: ${text}`)).toBe(text);
  });
  it('maps_the_change_link_sentences', () => {
    expect(syncErrorMessage('This is already the spec link.')).toBe(
      'This is already the spec link.',
    );
    expect(syncErrorMessage('This collection is not connected to an OpenAPI spec.')).toBe(
      'This collection is not connected to an OpenAPI spec.',
    );
    expect(syncErrorMessage('Changing the link failed. Nothing was changed. /home/u/x')).toBe(
      'Changing the link failed. Nothing was changed.',
    );
    expect(
      syncErrorMessage('An earlier sync is not finished. Try again after Rocket restarts.'),
    ).toBe('An earlier sync is not finished. Try again after Rocket restarts.');
    expect(isStaleConnect('The spec changed. Run the preview again.')).toBe(true);
  });

  it('passes_only_the_fixed_change_link_warnings', () => {
    expect(changeLinkWarningMessage('The token was not saved.')).toBe('The token was not saved.');
    expect(changeLinkWarningMessage('The old token could not be removed.')).toBe(
      'The old token could not be removed.',
    );
    expect(changeLinkWarningMessage('The new link needs your approval before checks run.')).toBe(
      'The new link needs your approval before checks run.',
    );
    expect(changeLinkWarningMessage('keychain said no at /home/u')).toBeNull();
  });
});
