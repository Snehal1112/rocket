import { describe, expect, it } from 'vitest';
import { parseGraphQlResponse } from '../graphql-response';

describe('parseGraphQlResponse', () => {
  it('reads data and no errors from a clean response', () => {
    const r = parseGraphQlResponse('{"data":{"a":1}}');
    expect(r.isGraphQl).toBe(true);
    expect(r.data).toEqual({ a: 1 });
    expect(r.errors).toEqual([]);
  });

  it('reads errors with path, locations and extensions', () => {
    const r = parseGraphQlResponse(
      JSON.stringify({
        data: null,
        errors: [
          {
            message: 'boom',
            path: ['user', 0, 'name'],
            locations: [{ line: 2, column: 3 }],
            extensions: { code: 'FORBIDDEN' },
          },
        ],
      }),
    );
    expect(r.errors).toHaveLength(1);
    expect(r.errors[0]).toMatchObject({
      message: 'boom',
      path: ['user', 0, 'name'],
      locations: [{ line: 2, column: 3 }],
      extensions: { code: 'FORBIDDEN' },
    });
  });

  it('treats a string error as a message', () => {
    expect(parseGraphQlResponse('{"errors":["plain"]}').errors[0].message).toBe('plain');
  });

  it('is not a graphql response for other bodies', () => {
    expect(parseGraphQlResponse('<html></html>').isGraphQl).toBe(false);
    expect(parseGraphQlResponse('[1,2]').isGraphQl).toBe(false);
    expect(parseGraphQlResponse('{"status":"ok"}').isGraphQl).toBe(false);
  });
});
