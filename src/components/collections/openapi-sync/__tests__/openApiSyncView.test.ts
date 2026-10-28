import { describe, expect, it } from 'vitest';
import {
  defaultChoice,
  displayPath,
  fieldLabel,
  splitLocation,
  filterItems,
  filterVariables,
  firstEntryId,
  groupItems,
  isFileLocation,
  isGithubLocation,
  isPendingChangedError,
  outcomeLabel,
  renameHint,
  isResetEligible,
  resetAllConfirmText,
  resetConfirmText,
} from '../openApiSyncView';
import { changedItem, makePlan, newItem, removedItem } from './fixtures';

describe('openApiSyncView', () => {
  it('display_path_drops_the_leading_variable', () => {
    expect(displayPath('{{baseUrl}}/pets/:id')).toBe('/pets/:id');
    expect(displayPath('https://x.test/pets')).toBe('https://x.test/pets');
    expect(displayPath('{{baseUrl}}')).toBe('/');
    expect(displayPath('')).toBe('');
  });

  it('the_reset_confirm_names_the_replaced_fields', () => {
    const text = resetConfirmText({ method: 'GET', url: '/orders', fields: ['url', 'name'] });
    expect(text).toContain('Reset GET /orders to the spec?');
    expect(text).toContain(
      'Your changes to the URL, parameters, headers, body and auth of this request will be replaced with the last synced version.',
    );
    expect(text).toContain('Your name, method, description, scripts, tests, assertions and docs are kept.');
    expect(text).toContain('A backup is kept so you can restore it.');
    expect(text).not.toContain('Everything the spec sets');
  });

  it('the_reset_all_confirm_uses_the_same_scope', () => {
    const text = resetAllConfirmText(3);
    expect(text).toContain('Reset 3 requests to the spec?');
    expect(text).toContain('URL, parameters, headers, body and auth');
    expect(resetAllConfirmText(1)).toContain('Reset 1 request to the spec?');
  });

  it('only_changes_to_reset_fields_make_a_row_reset_eligible', () => {
    expect(isResetEligible(['url'])).toBe(true);
    expect(isResetEligible(['header:Accept', 'name'])).toBe(true);
    expect(isResetEligible(['body:$.a'])).toBe(true);
    expect(isResetEligible(['form:x', 'query:limit', 'path:id', 'auth'])).toBe(true);
    expect(isResetEligible(['name'])).toBe(false);
    expect(isResetEligible(['name', 'method', 'description'])).toBe(false);
    // No field list means the row cannot say, so the backend decides.
    expect(isResetEligible([])).toBe(true);
  });

  it('groups New, Changed, Removed in that order with counts', () => {
    const groups = groupItems([removedItem, changedItem, newItem, { ...newItem, key: 'x' }]);
    expect(groups.map((g) => [g.label, g.items.length])).toEqual([
      ['New', 2],
      ['Changed', 1],
      ['Removed', 1],
    ]);
  });

  it('labels the three outcomes', () => {
    expect(outcomeLabel('takeSpec')).toBe('Spec wins');
    expect(outcomeLabel('keepYours')).toBe('Yours kept');
    expect(outcomeLabel('conflict')).toBe('Conflict');
  });

  it('builds rename hints for new and removed items', () => {
    expect(renameHint(newItem)).toBe('Possibly renamed from GET /animals');
    expect(renameHint(removedItem)).toBe('Possibly renamed to DELETE /animals');
    expect(renameHint(changedItem)).toBeNull();
  });

  it('shortens_a_link_with_credentials', () => {
    expect(splitLocation('https://user:secret@example.com/openapi.json')).toEqual({
      short: 'example.com/openapi.json',
      full: 'https://user:secret@example.com/openapi.json',
    });
    expect(splitLocation('https://example.com/openapi.json').full).toBeNull();
  });

  it('maps field names to display names', () => {
    expect(fieldLabel('header:Accept')).toBe('Header Accept');
    expect(fieldLabel('body:$.a.b')).toBe('Body $.a.b');
    expect(fieldLabel('url')).toBe('URL');
    expect(fieldLabel('name')).toBe('Name');
  });

  it('selects the first entry of the first non-empty group', () => {
    expect(firstEntryId(makePlan())).toBe('new:GET /pets');
    expect(firstEntryId(makePlan({ items: [] }))).toBeNull();
  });

  it('detects the pending-changed conflict', () => {
    expect(isPendingChangedError('Conflict: the pending spec changed, check again')).toBe(true);
    expect(isPendingChangedError(new Error('boom'))).toBe(false);
    expect(isPendingChangedError('Conflict: an earlier apply is not finished yet')).toBe(false);
  });

  it('defaults a choice from the outcome, conflicts go to yours', () => {
    expect(defaultChoice('takeSpec')).toBe('spec');
    expect(defaultChoice('keepYours')).toBe('yours');
    expect(defaultChoice('conflict')).toBe('yours');
  });

  it('filters items by name or key, ignoring case, and keeps all for an empty query', () => {
    const items = [newItem, changedItem, removedItem];
    expect(filterItems(items, '')).toHaveLength(3);
    expect(filterItems(items, '  ')).toHaveLength(3);
    expect(filterItems(items, 'add PET').map((i) => i.key)).toEqual(['POST /pets']);
    expect(filterItems(items, 'delete /pets').map((i) => i.key)).toEqual(['DELETE /pets']);
    expect(filterItems(items, 'zzz')).toEqual([]);
  });

  it('filters variable changes by label', () => {
    const vars = [
      { name: 'baseUrl', kind: 'baseUrl' as const, digest: 'a' },
      { name: 'apiKey', kind: 'addSecurity' as const, digest: 'b' },
    ];
    expect(filterVariables(vars, 'security').map((v) => v.name)).toEqual(['apiKey']);
    expect(filterVariables(vars, '')).toHaveLength(2);
  });
});

describe('isGithubLocation', () => {
  it('accepts_the_github_hosts_the_backend_treats_as_github', () => {
    expect(isGithubLocation('https://github.com/a/b/blob/main/x.yaml')).toBe(true);
    expect(isGithubLocation('https://raw.githubusercontent.com/a/b/main/x.yaml')).toBe(true);
    expect(isGithubLocation('https://api.github.com/repos/a/b/contents/x.yaml?ref=main')).toBe(
      true,
    );
    expect(isGithubLocation('https://example.com/x.yaml')).toBe(false);
    expect(isGithubLocation('https://github.com.evil.example/x.yaml')).toBe(false);
  });

  it('split_location_handles_a_localhost_link', () => {
    expect(splitLocation('http://localhost:3000/openapi.json')).toEqual({
      short: 'http://localhost:3000/openapi.json',
      full: null,
    });
    expect(splitLocation('http://127.0.0.1:8080/openapi.json?format=yaml')).toEqual({
      short: '127.0.0.1:8080/openapi.json',
      full: 'http://127.0.0.1:8080/openapi.json?format=yaml',
    });
  });

  it('a_localhost_link_is_not_github', () => {
    expect(isGithubLocation('http://localhost:3000/openapi.json')).toBe(false);
    expect(isGithubLocation('http://github.com/acme/api/blob/main/o.yaml')).toBe(false);
  });

  it('split_location_shows_the_file_name_and_folder', () => {
    expect(splitLocation('file:///a/b/api.yaml')).toEqual({
      short: 'api.yaml in /a/b',
      full: null,
    });
    expect(splitLocation('file:///api.yaml').short).toBe('api.yaml in /');
    expect(splitLocation('file:///C:/specs/api.yaml').short).toBe('api.yaml in C:/specs');
  });

  it('isFileLocation_recognises_the_file_form_only', () => {
    expect(isFileLocation('file:///a/b/api.yaml')).toBe(true);
    expect(isFileLocation('https://example.com/file://x')).toBe(false);
    expect(isFileLocation('http://localhost:3000/o.json')).toBe(false);
  });
});
