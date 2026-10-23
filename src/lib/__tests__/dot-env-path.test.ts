// src/lib/__tests__/dot-env-path.test.ts

import { describe, expect, it } from 'vitest';
import { toStoredDotEnvPath } from '../dot-env-path';

describe('toStoredDotEnvPath', () => {
  it('a_file_inside_the_base_is_stored_relative', () => {
    expect(toStoredDotEnvPath('/col/config/dev.env', '/col')).toBe('config/dev.env');
  });

  it('a_file_directly_in_the_base_is_stored_as_its_name', () => {
    expect(toStoredDotEnvPath('/col/.env.prod', '/col')).toBe('.env.prod');
  });

  it('a_file_outside_the_base_is_stored_absolute', () => {
    expect(toStoredDotEnvPath('/other/.env', '/col')).toBe('/other/.env');
  });

  it('a_sibling_folder_with_the_same_prefix_is_outside', () => {
    expect(toStoredDotEnvPath('/col2/.env', '/col')).toBe('/col2/.env');
  });

  it('windows_separators_are_handled', () => {
    expect(toStoredDotEnvPath('C:\\col\\a\\.env', 'C:\\col')).toBe('a/.env');
  });

  it('a_null_base_keeps_the_path_as_picked', () => {
    expect(toStoredDotEnvPath('/col/.env', null)).toBe('/col/.env');
  });

  it('a_base_with_a_trailing_separator_still_matches', () => {
    expect(toStoredDotEnvPath('/col/a/.env', '/col/')).toBe('a/.env');
  });

  it('a_parent_segment_keeps_the_path_as_picked', () => {
    expect(toStoredDotEnvPath('/col/../x', '/col')).toBe('/col/../x');
    expect(toStoredDotEnvPath('/col/a/../../x', '/col')).toBe('/col/a/../../x');
    expect(toStoredDotEnvPath('C:\\col\\..\\x', 'C:\\col')).toBe('C:\\col\\..\\x');
  });
});
