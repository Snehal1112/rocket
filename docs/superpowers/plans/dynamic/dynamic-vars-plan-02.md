# Dynamic Variables Plan 02: TypeScript Generator Registry

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the TypeScript dynamic variable registry using `@faker-js/faker`, with full Bruno parity (118 variables).

**Architecture:** A single `src/lib/dynamic-vars.ts` file exporting a registry map of variable name → generator function. Three public functions: `isDynamicVar`, `generateDynamicVar`, `listDynamicVars`.

**Tech Stack:** TypeScript, `@faker-js/faker`, Vitest

**Spec:** Before starting, read `docs/superpowers/specs/2026-04-21-dynamic-variables-design.md`.

---

### Task 1: Install faker.js and create the registry

**Files:**
- Create: `src/lib/dynamic-vars.ts`

- [ ] **Step 1: Install `@faker-js/faker`**

```bash
yarn add @faker-js/faker
```

- [ ] **Step 2: Create `src/lib/dynamic-vars.ts`**

```typescript
import { faker } from '@faker-js/faker';

/**
 * Registry of all Bruno-compatible dynamic variables.
 * Keys are variable names WITHOUT the $ prefix.
 * Each value is a generator function returning a fresh string.
 */
const DYNAMIC_VAR_REGISTRY: Record<string, () => string> = {
  // ── Basic Data Types ──
  guid: () => faker.string.uuid(),
  randomUUID: () => faker.string.uuid(),
  timestamp: () => Math.floor(Date.now() / 1000).toString(),
  isoTimestamp: () => new Date().toISOString(),
  randomNanoId: () => faker.string.nanoid(),
  randomAlphaNumeric: () => faker.string.alphanumeric(1),
  randomBoolean: () => faker.datatype.boolean().toString(),
  randomInt: () => faker.number.int({ min: 0, max: 1000 }).toString(),
  randomColor: () => faker.color.human(),
  randomHexColor: () => faker.color.rgb(),
  randomAbbreviation: () => faker.hacker.abbreviation(),
  randomWord: () => faker.lorem.word(),
  randomWords: () => faker.lorem.words(3),

  // ── Internet and Network ──
  randomIP: () => faker.internet.ipv4(),
  randomIPV4: () => faker.internet.ipv4(),
  randomIPV6: () => faker.internet.ipv6(),
  randomMACAddress: () => faker.internet.mac(),
  randomPassword: () => faker.internet.password({ length: 15 }),
  randomLocale: () => faker.location.countryCode('alpha-2').toLowerCase(),
  randomUserAgent: () => faker.internet.userAgent(),
  randomProtocol: () => faker.internet.protocol(),
  randomSemver: () => faker.system.semver(),
  randomDomainName: () => faker.internet.domainName(),
  randomDomainSuffix: () => faker.internet.domainSuffix(),
  randomDomainWord: () => faker.internet.domainWord(),
  randomExampleEmail: () => faker.internet.exampleEmail(),
  randomEmail: () => faker.internet.email(),
  randomUserName: () => faker.internet.username(),
  randomUrl: () => faker.internet.url(),

  // ── Names and Personal Information ──
  randomFirstName: () => faker.person.firstName(),
  randomLastName: () => faker.person.lastName(),
  randomFullName: () => faker.person.fullName(),
  randomNamePrefix: () => faker.person.prefix(),
  randomNameSuffix: () => faker.person.suffix(),
  randomJobArea: () => faker.person.jobArea(),
  randomJobDescriptor: () => faker.person.jobDescriptor(),
  randomJobTitle: () => faker.person.jobTitle(),
  randomJobType: () => faker.person.jobType(),
  randomPhoneNumber: () => faker.phone.number(),
  randomPhoneNumberExt: () => `${faker.phone.number()} ext. ${faker.number.int({ min: 100, max: 999 })}`,

  // ── Location ──
  randomCity: () => faker.location.city(),
  randomStreetName: () => faker.location.street(),
  randomStreetAddress: () => faker.location.streetAddress(),
  randomCountry: () => faker.location.country(),
  randomCountryCode: () => faker.location.countryCode(),
  randomLatitude: () => faker.location.latitude().toString(),
  randomLongitude: () => faker.location.longitude().toString(),

  // ── Images ──
  randomAvatarImage: () => `https://i.pravatar.cc/${faker.number.int({ min: 200, max: 400 })}`,
  randomImageUrl: () => faker.image.url(),
  randomAbstractImage: () => 'https://loremflickr.com/320/240/abstract',
  randomAnimalsImage: () => 'https://loremflickr.com/320/240/animals',
  randomBusinessImage: () => 'https://loremflickr.com/320/240/business',
  randomCatsImage: () => 'https://loremflickr.com/320/240/cats',
  randomCityImage: () => 'https://loremflickr.com/320/240/city',
  randomFoodImage: () => 'https://loremflickr.com/320/240/food',
  randomNightlifeImage: () => 'https://loremflickr.com/320/240/nightlife',
  randomFashionImage: () => 'https://loremflickr.com/320/240/fashion',
  randomPeopleImage: () => 'https://loremflickr.com/320/240/people',
  randomNatureImage: () => 'https://loremflickr.com/320/240/nature',
  randomSportsImage: () => 'https://loremflickr.com/320/240/sports',
  randomTransportImage: () => 'https://loremflickr.com/320/240/transport',
  randomImageDataUri: () => faker.image.dataUri({ width: 1, height: 1 }),

  // ── Finance ──
  randomBankAccount: () => faker.finance.accountNumber(10),
  randomBankAccountName: () => faker.finance.accountName(),
  randomCreditCardMask: () => faker.finance.maskedNumber(),
  randomBankAccountBic: () => faker.finance.bic(),
  randomBankAccountIban: () => faker.finance.iban(),
  randomTransactionType: () => faker.finance.transactionType(),
  randomCurrencyCode: () => faker.finance.currencyCode(),
  randomCurrencyName: () => faker.finance.currencyName(),
  randomCurrencySymbol: () => faker.finance.currencySymbol(),
  randomBitcoin: () => faker.finance.bitcoinAddress(),

  // ── Business ──
  randomCompanyName: () => faker.company.name(),
  randomCompanySuffix: () => faker.company.companySuffix(),
  randomBs: () => faker.company.buzzPhrase(),
  randomBsAdjective: () => faker.company.buzzAdjective(),
  randomBsBuzz: () => faker.company.buzzVerb(),
  randomBsNoun: () => faker.company.buzzNoun(),
  randomCatchPhrase: () => faker.company.catchPhrase(),
  randomCatchPhraseAdjective: () => faker.company.catchPhraseAdjective(),
  randomCatchPhraseDescriptor: () => faker.company.catchPhraseDescriptor(),
  randomCatchPhraseNoun: () => faker.company.catchPhraseNoun(),

  // ── Database ──
  randomDatabaseColumn: () => faker.database.column(),
  randomDatabaseType: () => faker.database.type(),
  randomDatabaseCollation: () => faker.database.collation(),
  randomDatabaseEngine: () => faker.database.engine(),

  // ── Dates ──
  randomDateFuture: () => faker.date.future().toISOString(),
  randomDatePast: () => faker.date.past().toISOString(),
  randomDateRecent: () => faker.date.recent().toISOString(),
  randomWeekday: () => faker.date.weekday(),
  randomMonth: () => faker.date.month(),

  // ── Files and System ──
  randomFileName: () => faker.system.fileName(),
  randomFileType: () => faker.system.fileType(),
  randomFileExt: () => faker.system.fileExt(),
  randomCommonFileName: () => faker.system.commonFileName(),
  randomCommonFileType: () => faker.system.commonFileType(),
  randomCommonFileExt: () => faker.system.commonFileExt(),
  randomFilePath: () => faker.system.filePath(),
  randomDirectoryPath: () => faker.system.directoryPath(),
  randomMimeType: () => faker.system.mimeType(),

  // ── Commerce ──
  randomPrice: () => faker.commerce.price(),
  randomProduct: () => faker.commerce.product(),
  randomProductAdjective: () => faker.commerce.productAdjective(),
  randomProductMaterial: () => faker.commerce.productMaterial(),
  randomProductName: () => faker.commerce.productName(),
  randomDepartment: () => faker.commerce.department(),

  // ── Hacker and Lorem ──
  randomNoun: () => faker.hacker.noun(),
  randomVerb: () => faker.hacker.verb(),
  randomIngverb: () => faker.hacker.ingverb(),
  randomAdjective: () => faker.hacker.adjective(),
  randomPhrase: () => faker.hacker.phrase(),
  randomLoremWord: () => faker.lorem.word(),
  randomLoremWords: () => faker.lorem.words(),
  randomLoremSentence: () => faker.lorem.sentence(),
  randomLoremSentences: () => faker.lorem.sentences(),
  randomLoremParagraph: () => faker.lorem.paragraph(),
  randomLoremParagraphs: () => faker.lorem.paragraphs(),
  randomLoremText: () => faker.lorem.text(),
  randomLoremSlug: () => faker.lorem.slug(),
  randomLoremLines: () => faker.lorem.lines(),
};

/** Check if a variable name (without $) is a known dynamic variable. */
export function isDynamicVar(name: string): boolean {
  return name in DYNAMIC_VAR_REGISTRY;
}

/** Generate a value for a dynamic variable. Returns undefined if unknown. */
export function generateDynamicVar(name: string): string | undefined {
  return DYNAMIC_VAR_REGISTRY[name]?.();
}

/** List all supported dynamic variable names (for autocomplete/docs). */
export function listDynamicVars(): string[] {
  return Object.keys(DYNAMIC_VAR_REGISTRY);
}
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/dynamic-vars.ts package.json yarn.lock
git commit -m "feat: add TypeScript dynamic variable registry (118 variables, faker.js)"
```

---

### Task 2: Write unit tests for dynamic-vars.ts

**Files:**
- Create: `src/lib/__tests__/dynamic-vars.test.ts`

- [ ] **Step 1: Create the test file**

```typescript
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
  it('returns all 118 variable names', () => {
    const vars = listDynamicVars();
    expect(vars.length).toBe(118);
  });

  it('includes key variables from every category', () => {
    const vars = listDynamicVars();
    // One from each category
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
```

- [ ] **Step 2: Run the tests**

```bash
npx vitest run src/lib/__tests__/dynamic-vars.test.ts
```

Expected: all tests PASS. If any faker.js API method doesn't exist (e.g. `companySuffix` was removed in newer versions), fix the specific registry entry.

- [ ] **Step 3: Commit**

```bash
git add src/lib/__tests__/dynamic-vars.test.ts
git commit -m "test: add dynamic-vars.ts unit tests — all 118 variables verified"
```
