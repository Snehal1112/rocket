import { describe, expect, it } from 'vitest';
import { findOperationLine, splitLines } from '../specSourceLines';

const YAML = `openapi: 3.0.0
info:
  title: Pets
paths:
  /pets:
    get:
      summary: List
    post:
      summary: Add
  '/pets/{id}':
    get:
      summary: One
    delete:
      summary: Remove
  "/v1/a.b":
    put:
      summary: Dot
  /v1/axb:
    put:
      summary: Other
components:
  schemas:
    get:
      type: string
`;

const lines = (t: string) => splitLines(t);

describe('findOperationLine yaml', () => {
  it('finds_a_bare_path_method', () => {
    expect(findOperationLine(lines(YAML), 'yaml', '/pets', 'GET')).toBe(6);
    expect(findOperationLine(lines(YAML), 'yaml', '/pets', 'post')).toBe(8);
  });

  it('finds_quoted_paths_with_braces', () => {
    expect(findOperationLine(lines(YAML), 'yaml', '/pets/{id}', 'get')).toBe(11);
    expect(findOperationLine(lines(YAML), 'yaml', '/pets/{id}', 'delete')).toBe(13);
  });

  it('does_not_treat_a_dot_as_a_wildcard', () => {
    expect(findOperationLine(lines(YAML), 'yaml', '/v1/a.b', 'put')).toBe(16);
    expect(findOperationLine(lines(YAML), 'yaml', '/v1/axb', 'put')).toBe(19);
  });

  it('a_prefix_path_does_not_match_a_longer_one', () => {
    expect(findOperationLine(lines(YAML), 'yaml', '/pets', 'delete')).toBeNull();
  });

  it('returns_null_for_a_missing_method_or_path', () => {
    expect(findOperationLine(lines(YAML), 'yaml', '/pets', 'patch')).toBeNull();
    expect(findOperationLine(lines(YAML), 'yaml', '/nope', 'get')).toBeNull();
  });

  it('ignores_keys_outside_the_paths_block', () => {
    expect(findOperationLine(lines(YAML), 'yaml', '/inventory', 'get')).toBeNull();
  });

  it('handles_crlf_line_endings', () => {
    expect(findOperationLine(lines(YAML.replace(/\n/g, '\r\n')), 'yaml', '/pets/{id}', 'get')).toBe(11);
  });
});

describe('findOperationLine json', () => {
  const JSON_TEXT = `{
  "openapi": "3.0.0",
  "paths": {
    "/pets": {
      "get": {
        "summary": "List"
      }
    },
    "/pets/{id}": {
      "get": {},
      "delete": {}
    }
  }
}`;

  it('finds_the_method_under_the_path', () => {
    expect(findOperationLine(lines(JSON_TEXT), 'json', '/pets', 'GET')).toBe(5);
    expect(findOperationLine(lines(JSON_TEXT), 'json', '/pets/{id}', 'delete')).toBe(11);
  });

  it('returns_null_when_missing', () => {
    expect(findOperationLine(lines(JSON_TEXT), 'json', '/pets', 'delete')).toBeNull();
    expect(findOperationLine(lines(JSON_TEXT), 'json', '/x', 'get')).toBeNull();
  });

  it('ignores_a_deeper_get_property_in_a_sibling', () => {
    const text = `{
  "paths": {
    "/a": {
      "post": {
        "example": {
          "get": 1
        }
      }
    },
    "/b": {
      "get": {}
    }
  }
}`;
    expect(findOperationLine(lines(text), 'json', '/a', 'get')).toBeNull();
    expect(findOperationLine(lines(text), 'json', '/b', 'get')).toBe(11);
  });

  it('gives_null_for_minified_json', () => {
    const text = '{"openapi":"3.0.0","paths":{"/pets":{"get":{}}}}';
    expect(findOperationLine(lines(text), 'json', '/pets', 'get')).toBeNull();
  });
});

describe('findOperationLine yaml paths with a colon', () => {
  const COLON = `paths:
  /v1/{name}:cancel:
    post:
      summary: Cancel
  '/v1/{name}:cancel:':
    get:
      summary: Quoted
  "/v1/{name}:go":
    put:
      summary: Double
  /v1/{name}:
    delete:
      summary: Prefix
  /v1/{name}:cancel2:
    patch:
      summary: Longer
`;

  it('finds_a_bare_path_with_a_colon_in_the_middle', () => {
    expect(findOperationLine(lines(COLON), 'yaml', '/v1/{name}:cancel', 'post')).toBe(3);
  });

  it('finds_a_quoted_path_with_a_colon', () => {
    expect(findOperationLine(lines(COLON), 'yaml', '/v1/{name}:cancel:', 'get')).toBe(6);
    expect(findOperationLine(lines(COLON), 'yaml', '/v1/{name}:go', 'put')).toBe(9);
  });

  it('a_prefix_path_does_not_match_a_longer_colon_path', () => {
    expect(findOperationLine(lines(COLON), 'yaml', '/v1/{name}', 'delete')).toBe(12);
    expect(findOperationLine(lines(COLON), 'yaml', '/v1/{name}', 'patch')).toBeNull();
    expect(findOperationLine(lines(COLON), 'yaml', '/v1/{name}:cancel2', 'patch')).toBe(15);
  });
});
