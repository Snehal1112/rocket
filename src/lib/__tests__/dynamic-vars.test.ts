import { describe, expect, it } from 'vitest';
import { generateDynamicVar, isDynamicVar, listDynamicVars } from '../dynamic-vars';

/**
 * Assert the value is defined and return it as string.
 * Removes the need for non-null assertions at every call site.
 */
function generateOrFail(name: string): string {
  const val = generateDynamicVar(name);
  if (val === undefined) throw new Error(`generateDynamicVar('${name}') returned undefined.`);
  return val;
}

describe('isDynamicVar', () => {
  it('returns true for known variables', () => {
    expect(isDynamicVar('guid')).toBe(true);
    expect(isDynamicVar('randomUUID')).toBe(true);
    expect(isDynamicVar('randomEmail')).toBe(true);
    expect(isDynamicVar('timestamp')).toBe(true);
    expect(isDynamicVar('randomLoremParagraph')).toBe(true);
  });

  it('returns false for unknown variables', () => {
    expect(isDynamicVar('unknownThing')).toBe(false);
    expect(isDynamicVar('')).toBe(false);
    // Case-sensitive lookup.
    expect(isDynamicVar('GUID')).toBe(false);
  });
});

describe('generateDynamicVar', () => {
  it('returns a string for known variables', () => {
    const val = generateDynamicVar('guid');
    expect(val).toBeDefined();
    expect(typeof val).toBe('string');
    expect(val?.length ?? 0).toBeGreaterThan(0);
  });

  it('returns undefined for unknown variables', () => {
    expect(generateDynamicVar('doesNotExist')).toBeUndefined();
  });

  it('guid produces valid UUID format', () => {
    const val = generateOrFail('guid');
    expect(val).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
  });

  it('timestamp produces a valid unix epoch', () => {
    const val = generateOrFail('timestamp');
    const num = parseInt(val, 10);
    // After ~2001.
    expect(num).toBeGreaterThan(1000000000);
    // Before ~2286.
    expect(num).toBeLessThan(9999999999);
  });

  it('isoTimestamp produces valid ISO 8601', () => {
    const val = generateOrFail('isoTimestamp');
    const date = new Date(val);
    expect(date.toISOString()).toBe(val);
  });

  it('randomInt is within 0-1000', () => {
    for (let i = 0; i < 20; i++) {
      const val = parseInt(generateOrFail('randomInt'), 10);
      expect(val).toBeGreaterThanOrEqual(0);
      expect(val).toBeLessThanOrEqual(1000);
    }
  });

  it('randomBoolean is "true" or "false"', () => {
    for (let i = 0; i < 20; i++) {
      const val = generateOrFail('randomBoolean');
      expect(['true', 'false']).toContain(val);
    }
  });

  it('randomEmail contains @', () => {
    const val = generateOrFail('randomEmail');
    expect(val).toContain('@');
  });

  it('randomProtocol returns http or https', () => {
    for (let i = 0; i < 10; i++) {
      const val = generateOrFail('randomProtocol');
      expect(['http', 'https']).toContain(val);
    }
  });

  it('two calls to guid produce different values', () => {
    const vals = Array.from({ length: 10 }, () => generateDynamicVar('guid'));
    const unique = new Set(vals);
    expect(unique.size).toBeGreaterThan(1);
  });
});

describe('listDynamicVars', () => {
  it('returns all 120 variable names', () => {
    const vars = listDynamicVars();
    expect(vars.length).toBe(120);
  });

  it('includes key variables from every category', () => {
    const vars = listDynamicVars();
    expect(vars).toContain('guid');
    expect(vars).toContain('randomEmail');
    expect(vars).toContain('randomFirstName');
    expect(vars).toContain('randomCity');
    expect(vars).toContain('randomImageUrl');
    expect(vars).toContain('randomBankAccount');
    expect(vars).toContain('randomCompanyName');
    expect(vars).toContain('randomDatabaseColumn');
    expect(vars).toContain('randomDateFuture');
    expect(vars).toContain('randomFileName');
    expect(vars).toContain('randomPrice');
    expect(vars).toContain('randomLoremWord');
  });
});

describe('all registered variables generate without error', () => {
  const allVars = listDynamicVars();
  for (const name of allVars) {
    it(`generates a value for "${name}"`, () => {
      const val = generateDynamicVar(name);
      expect(val).toBeDefined();
      expect(typeof val).toBe('string');
      expect(val?.length ?? 0).toBeGreaterThan(0);
    });
  }
});
