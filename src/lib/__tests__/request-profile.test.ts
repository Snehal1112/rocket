import { describe, expect, it } from 'vitest';
import { requestProfile } from '../request-profile';

describe('requestProfile', () => {
  it('keeps the full HTTP surface for http', () => {
    const p = requestProfile('http');
    expect(p.methods).toEqual(['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS', 'HEAD']);
    expect(p.bodyTabLabel).toBe('Body');
    expect(p.showLoadTest).toBe(true);
    expect(p.showCopyAsCurl).toBe(true);
    expect(p.initialSection).toBe('params');
  });

  it('limits graphql to POST and GET and hides what only fits HTTP bodies', () => {
    const p = requestProfile('graphql');
    expect(p.methods).toEqual(['POST', 'GET']);
    expect(p.bodyTabLabel).toBe('Query');
    expect(p.showLoadTest).toBe(false);
    expect(p.showCopyAsCurl).toBe(false);
    expect(p.initialSection).toBe('body');
  });
});
