import { describe, expect, it } from 'vitest';
import { generateDynamicVar, isDynamicVar, listDynamicVars } from '../dynamic-vars';

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
    expect(isDynamicVar('GUID')).toBe(false); // case-sensitive
  });
});

describe('generateDynamicVar', () => {
  it('returns a string for known variables', () => {
    const val = generateDynamicVar('guid');
    expect(val).toBeDefined();
    expect(typeof val).toBe('string');
    expect(val!.length).toBeGreaterThan(0);
  });

  it('returns undefined for unknown variables', () => {
    expect(generateDynamicVar('doesNotExist')).toBeUndefined();
  });

  it('guid produces valid UUID format', () => {
    const val = generateDynamicVar('guid')!;
    expect(val).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
  });

  it('timestamp produces a valid unix epoch', () => {
    const val = generateDynamicVar('timestamp')!;
    const num = parseInt(val, 10);
    expect(num).toBeGreaterThan(1000000000); // after ~2001
    expect(num).toBeLessThan(9999999999);    // before ~2286
  });

  it('isoTimestamp produces valid ISO 8601', () => {
    const val = generateDynamicVar('isoTimestamp')!;
    const date = new Date(val);
    expect(date.toISOString()).toBe(val);
  });

  it('randomInt is within 0-1000', () => {
    for (let i = 0; i < 20; i++) {
      const val = parseInt(generateDynamicVar('randomInt')!, 10);
      expect(val).toBeGreaterThanOrEqual(0);
      expect(val).toBeLessThanOrEqual(1000);
    }
  });

  it('randomBoolean is "true" or "false"', () => {
    for (let i = 0; i < 20; i++) {
      const val = generateDynamicVar('randomBoolean')!;
      expect(['true', 'false']).toContain(val);
    }
  });

  it('randomEmail contains @', () => {
    const val = generateDynamicVar('randomEmail')!;
    expect(val).toContain('@');
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
    expect(vars).toContain('guid');           // Basic
    expect(vars).toContain('randomEmail');     // Internet
    expect(vars).toContain('randomFirstName'); // Names
    expect(vars).toContain('randomCity');      // Location
    expect(vars).toContain('randomImageUrl');  // Images
    expect(vars).toContain('randomBankAccount'); // Finance
    expect(vars).toContain('randomCompanyName'); // Business
    expect(vars).toContain('randomDatabaseColumn'); // Database
    expect(vars).toContain('randomDateFuture');     // Dates
    expect(vars).toContain('randomFileName');        // Files
    expect(vars).toContain('randomPrice');            // Commerce
    expect(vars).toContain('randomLoremWord');        // Lorem
  });
});

describe('all registered variables generate without error', () => {
  const allVars = listDynamicVars();
  for (const name of allVars) {
    it(`generates a value for "${name}"`, () => {
      const val = generateDynamicVar(name);
      expect(val).toBeDefined();
      expect(typeof val).toBe('string');
      expect(val!.length).toBeGreaterThan(0);
    });
  }
});
