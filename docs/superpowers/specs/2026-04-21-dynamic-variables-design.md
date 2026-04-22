# Dynamic Variables — Design Spec

**Date:** 2026-04-21
**Status:** Approved
**Feature:** Bruno-compatible `$`-prefixed dynamic variables for RocketAPI

---

## Overview

Dynamic variables (e.g. `{{$guid}}`, `{{$randomEmail}}`, `{{$randomInt}}`) generate fresh
random or time-based data on every resolution. They use the same `{{...}}` mustache syntax
as regular variables but are distinguished by the `$` prefix. They are never stored in any
scope or written to disk — they exist only in resolution output at send time or UI preview time.

Bruno uses faker.js for ~118 dynamic variables across 12 categories. RocketAPI will achieve
full parity with all 118 variable names.

---

## Architecture

### Two-Layer Generation

```
Frontend (TypeScript)                    Backend (Rust)
┌─────────────────────────┐              ┌─────────────────────────┐
│ @faker-js/faker          │              │ fake crate + uuid +     │
│                         │              │ chrono + rand            │
│ Used for:               │              │                         │
│ • URL bar previews      │              │ Used for:               │
│ • Variable popovers     │              │ • Actual send-time      │
│ • VariableAwareInput    │              │   resolution            │
│   overlay hints         │              │ • Load test generation  │
│                         │              │ • CLI execution         │
│ src/lib/dynamic-vars.ts │              │ rocket-environment/     │
│                         │              │   src/dynamic_vars.rs   │
└─────────────────────────┘              └─────────────────────────┘
```

Both layers share the same 118 variable names. The frontend uses `@faker-js/faker` for
instant UI previews. The backend uses the Rust `fake` crate for actual send-time resolution,
keeping it fast, dependency-free of JS runtimes, and safe for load testing.

Output is **functionally identical** (same categories, realistic data) but not
**byte-identical** (different underlying dictionaries). This is acceptable — nobody
compares faker output between tools.

### Resolution Flow

```
Template: "{{baseUrl}}/users/{{$randomUUID}}"
                │                    │
                ▼                    ▼
    Normal variable lookup    Dynamic variable generator
    (7-scope hierarchy)       ($-prefix → fake crate / faker.js)
                │                    │
                ▼                    ▼
    "https://api.example.com" + "/users/" + "f47ac10b-58cc-4372-a567-0e02b2c3d479"
```

### Key Behaviours

1. `$` prefix is the discriminator — checked **before** user variable lookup
2. User variables named `$guid` are shadowed — `{{$guid}}` always generates, never looks up
3. Each `{{$guid}}` occurrence produces a **fresh value** — two in the same template yield two different UUIDs
4. Unknown `$` variables (e.g. `{{$doesNotExist}}`) are left as-is and reported as unresolved
5. Dynamic variables are **never persisted** — the `{{$guid}}` text stays in `.yml` files on disk

---

## Rust Backend

### New file: `crates/rocket-environment/src/dynamic_vars.rs`

**Dependencies** (added to `rocket-environment/Cargo.toml`):
- `fake = { version = "3", features = ["derive"] }` — realistic data generation
- `uuid = { version = "1", features = ["v4"] }` — for `$guid` / `$randomUUID`
- `chrono = "0.4"` — for `$timestamp` / `$isoTimestamp`
- `rand = "0.8"` — for `$randomInt`, `$randomBoolean`, seeding

**Public API:**

```rust
/// Check if a name (without $ prefix) is a known dynamic variable.
pub fn is_dynamic_var(name: &str) -> bool;

/// Generate a fresh value for a dynamic variable. Returns None if unknown.
pub fn generate(name: &str) -> Option<String>;
```

**Generator structure** — a single `match` over all 118 names:

```rust
pub fn generate(name: &str) -> Option<String> {
    let mut rng = rand::thread_rng();
    match name {
        // Basic Data Types
        "guid" | "randomUUID"          => Some(uuid::Uuid::new_v4().to_string()),
        "timestamp"                    => Some(chrono::Utc::now().timestamp().to_string()),
        "isoTimestamp"                 => Some(chrono::Utc::now().to_rfc3339()),
        "randomNanoId"                 => Some(nanoid_generate()),
        "randomAlphaNumeric"           => Some(fake::faker::lorem::en::Word().fake_with_rng(&mut rng)),
        "randomBoolean"                => Some(rng.gen_bool(0.5).to_string()),
        "randomInt"                    => Some(rng.gen_range(0..=1000).to_string()),
        "randomColor"                  => Some(fake::faker::color::en::Color().fake_with_rng(&mut rng)),
        "randomHexColor"               => Some(format!("#{:06x}", rng.gen::<u32>() & 0xFFFFFF)),
        // ... all 118 entries
        _ => None,
    }
}
```

### Resolver integration: `crates/rocket-environment/src/resolver.rs`

Minimal change to the existing `resolve()` function. After extracting and trimming
the variable name, add a `$` prefix check before the user variable lookup:

```rust
if found_closing {
    if let Some(stripped) = var_name_trimmed.strip_prefix('$') {
        // Dynamic variable — generate fresh value
        if let Some(generated) = dynamic_vars::generate(stripped) {
            output.push_str(&generated);
        } else {
            // Unknown $variable — leave as-is, mark unresolved
            output.push_str("{{");
            output.push_str(&var_name);
            output.push_str("}}");
            unresolved.push(var_name_trimmed);
        }
    } else if let Some(value) = variables.get(&var_name_trimmed) {
        output.push_str(value);
    } else {
        // existing unresolved handling unchanged
        output.push_str("{{");
        output.push_str(&var_name);
        output.push_str("}}");
        unresolved.push(var_name_trimmed);
    }
}
```

### Module registration

Add `pub mod dynamic_vars;` to `crates/rocket-environment/src/lib.rs`.

---

## TypeScript Frontend

### New file: `src/lib/dynamic-vars.ts`

**Dependency**: `@faker-js/faker` (add via `yarn add @faker-js/faker`)

**Public API:**

```typescript
import { faker } from '@faker-js/faker';

const DYNAMIC_VAR_REGISTRY: Record<string, () => string> = {
  // Basic Data Types
  guid:                () => faker.string.uuid(),
  randomUUID:          () => faker.string.uuid(),
  timestamp:           () => Math.floor(Date.now() / 1000).toString(),
  isoTimestamp:        () => new Date().toISOString(),
  randomNanoId:        () => faker.string.nanoid(),
  randomAlphaNumeric:  () => faker.string.alphanumeric(1),
  randomBoolean:       () => faker.datatype.boolean().toString(),
  randomInt:           () => faker.number.int({ min: 0, max: 1000 }).toString(),
  randomColor:         () => faker.color.human(),
  randomHexColor:      () => faker.color.rgb(),
  randomAbbreviation:  () => faker.hacker.abbreviation(),
  randomWord:          () => faker.lorem.word(),
  randomWords:         () => faker.lorem.words(3),
  // ... all 118 entries
};

export function isDynamicVar(name: string): boolean {
  return name in DYNAMIC_VAR_REGISTRY;
}

export function generateDynamicVar(name: string): string | undefined {
  return DYNAMIC_VAR_REGISTRY[name]?.();
}

export function listDynamicVars(): string[] {
  return Object.keys(DYNAMIC_VAR_REGISTRY);
}
```

### Modified: `src/lib/variable-context.ts`

**Regex update** — allow `$` as first character:

```typescript
const VAR_REGEX = /\{\{\s*([\$\w.-]+)\s*\}\}/g;
```

**`resolveWithContext` update** — `$` prefix check before user lookup:

```typescript
import { generateDynamicVar } from './dynamic-vars';

export function resolveWithContext(template: string, ctx: Record<string, string>): string {
  return template.replace(VAR_REGEX, (match, key) => {
    if (key.startsWith('$')) {
      return generateDynamicVar(key.slice(1)) ?? match;
    }
    return key in ctx ? ctx[key] : match;
  });
}
```

### Modified: `src/lib/url-variables.ts`

**New source type:**

```typescript
export type VariableSource =
  | 'environment' | 'collection' | 'global' | 'folder'
  | 'request' | 'process' | 'runtime' | 'dynamic';
```

**Token parsing** — in `parseUrlTokens()`, when a `$`-prefixed variable is
encountered, set `source: 'dynamic'` and generate a preview value:

```typescript
import { generateDynamicVar, isDynamicVar } from './dynamic-vars';

// Inside the variable token creation logic:
if (varName.startsWith('$')) {
  const stripped = varName.slice(1);
  if (isDynamicVar(stripped)) {
    resolved = generateDynamicVar(stripped);
    source = 'Dynamic';
  }
  // else: unknown $var — resolved stays undefined, shown as unresolved
}
```

**`buildScopedContext` update** — no change needed here; dynamic variables
are not added to the scope map. They're resolved inline during token parsing
and template resolution. The popover detects them by checking for `$` prefix
in the variable name token.

**Badge colour for `'dynamic'` source:**

```typescript
// Add to sourceBadgeClass():
case 'dynamic': return 'bg-cyan-500/15 text-cyan-600 dark:text-cyan-400';
```

Badge letter: **D** (Dynamic).

### UI behaviour in popovers

When a `{{$randomEmail}}` token is hovered in the URL bar, VariableAwareInput,
or Monaco decorations:

- **Preview value**: generated fresh on each popover open (e.g. `"john.doe@example.com"`)
- **Source badge**: **D** (Dynamic) in cyan
- **Edit field**: none — read-only display
- **Navigate link**: none — no source to navigate to
- **Label**: `"Dynamic"`

---

## Complete Variable Registry (118 variables)

### Basic Data Types (13)
| Variable | Rust (`fake` crate / custom) | TypeScript (`@faker-js/faker`) |
|---|---|---|
| `guid` | `uuid::Uuid::new_v4()` | `faker.string.uuid()` |
| `timestamp` | `chrono::Utc::now().timestamp()` | `Math.floor(Date.now() / 1000)` |
| `isoTimestamp` | `chrono::Utc::now().to_rfc3339()` | `new Date().toISOString()` |
| `randomUUID` | `uuid::Uuid::new_v4()` | `faker.string.uuid()` |
| `randomNanoId` | custom: 21-char alphanumeric | `faker.string.nanoid()` |
| `randomAlphaNumeric` | `rng.sample(Alphanumeric)` | `faker.string.alphanumeric(1)` |
| `randomBoolean` | `rng.gen_bool(0.5)` | `faker.datatype.boolean()` |
| `randomInt` | `rng.gen_range(0..=1000)` | `faker.number.int({ min: 0, max: 1000 })` |
| `randomColor` | `Color().fake()` | `faker.color.human()` |
| `randomHexColor` | `format!("#{:06x}", ...)` | `faker.color.rgb()` |
| `randomAbbreviation` | `Buzzword().fake()` (approx) | `faker.hacker.abbreviation()` |
| `randomWord` | `Word().fake()` | `faker.lorem.word()` |
| `randomWords` | `Words(3..5).fake()` | `faker.lorem.words(3)` |

### Internet and Network (16)
| Variable | Rust | TypeScript |
|---|---|---|
| `randomIP` | `IPv4().fake()` | `faker.internet.ipv4()` |
| `randomIPV4` | `IPv4().fake()` | `faker.internet.ipv4()` |
| `randomIPV6` | `IPv6().fake()` | `faker.internet.ipv6()` |
| `randomMACAddress` | `MACAddress().fake()` | `faker.internet.mac()` |
| `randomPassword` | `Password(15..16).fake()` | `faker.internet.password({ length: 15 })` |
| `randomLocale` | custom: random from locale list | `faker.location.countryCode('alpha-2')` |
| `randomUserAgent` | `UserAgent().fake()` | `faker.internet.userAgent()` |
| `randomProtocol` | `random choice ["http","https"]` | `faker.internet.protocol()` |
| `randomSemver` | `Semver().fake()` | `faker.system.semver()` |
| `randomDomainName` | `DomainSuffix().fake()` | `faker.internet.domainName()` |
| `randomDomainSuffix` | custom: random from [.com,.org,.net] | `faker.internet.domainSuffix()` |
| `randomDomainWord` | `Word().fake()` | `faker.internet.domainWord()` |
| `randomExampleEmail` | `SafeEmail().fake()` | `faker.internet.exampleEmail()` |
| `randomEmail` | `FreeEmail().fake()` | `faker.internet.email()` |
| `randomUserName` | `Username().fake()` | `faker.internet.username()` |
| `randomUrl` | `format!("https://{}", DomainName)` | `faker.internet.url()` |

### Names and Personal Information (11)
| Variable | Rust | TypeScript |
|---|---|---|
| `randomFirstName` | `FirstName().fake()` | `faker.person.firstName()` |
| `randomLastName` | `LastName().fake()` | `faker.person.lastName()` |
| `randomFullName` | `Name().fake()` | `faker.person.fullName()` |
| `randomNamePrefix` | `Prefix().fake()` | `faker.person.prefix()` |
| `randomNameSuffix` | `Suffix().fake()` | `faker.person.suffix()` |
| `randomJobArea` | `Field().fake()` | `faker.person.jobArea()` |
| `randomJobDescriptor` | `JobDescriptor().fake()` (custom) | `faker.person.jobDescriptor()` |
| `randomJobTitle` | `Title().fake()` | `faker.person.jobTitle()` |
| `randomJobType` | `JobType().fake()` (custom) | `faker.person.jobType()` |
| `randomPhoneNumber` | `PhoneNumber().fake()` | `faker.phone.number()` |
| `randomPhoneNumberExt` | custom: phone + " ext. " + 3 digits | `faker.phone.number() + ext` |

### Location (7)
| Variable | Rust | TypeScript |
|---|---|---|
| `randomCity` | `CityName().fake()` | `faker.location.city()` |
| `randomStreetName` | `StreetName().fake()` | `faker.location.street()` |
| `randomStreetAddress` | `StreetAddress().fake()` (custom) | `faker.location.streetAddress()` |
| `randomCountry` | `CountryName().fake()` | `faker.location.country()` |
| `randomCountryCode` | `CountryCode().fake()` | `faker.location.countryCode()` |
| `randomLatitude` | `Latitude().fake()` | `faker.location.latitude().toString()` |
| `randomLongitude` | `Longitude().fake()` | `faker.location.longitude().toString()` |

### Images (14)
All image variables return placeholder URLs. Rust uses `format!()` with template
URLs; TypeScript uses `faker.image.*()` where available or matching format strings.

| Variable | Output pattern |
|---|---|
| `randomAvatarImage` | `https://i.pravatar.cc/300` |
| `randomImageUrl` | `https://picsum.photos/200/300` |
| `randomAbstractImage` | `https://loremflickr.com/320/240/abstract` |
| `randomAnimalsImage` | `https://loremflickr.com/320/240/animals` |
| `randomBusinessImage` | `https://loremflickr.com/320/240/business` |
| `randomCatsImage` | `https://loremflickr.com/320/240/cats` |
| `randomCityImage` | `https://loremflickr.com/320/240/city` |
| `randomFoodImage` | `https://loremflickr.com/320/240/food` |
| `randomNightlifeImage` | `https://loremflickr.com/320/240/nightlife` |
| `randomFashionImage` | `https://loremflickr.com/320/240/fashion` |
| `randomPeopleImage` | `https://loremflickr.com/320/240/people` |
| `randomNatureImage` | `https://loremflickr.com/320/240/nature` |
| `randomSportsImage` | `https://loremflickr.com/320/240/sports` |
| `randomTransportImage` | `https://loremflickr.com/320/240/transport` |
| `randomImageDataUri` | `data:image/png;base64,...` (tiny 1x1 pixel) |

### Finance (10)
| Variable | Rust | TypeScript |
|---|---|---|
| `randomBankAccount` | custom: 10-digit number string | `faker.finance.accountNumber(10)` |
| `randomBankAccountName` | custom: random from list | `faker.finance.accountName()` |
| `randomCreditCardMask` | `format!("**** **** **** {}", 4 digits)` | `faker.finance.maskedNumber()` |
| `randomBankAccountBic` | custom: 8-char alpha | `faker.finance.bic()` |
| `randomBankAccountIban` | custom: country + check + digits | `faker.finance.iban()` |
| `randomTransactionType` | custom: random from [deposit, withdrawal, ...] | `faker.finance.transactionType()` |
| `randomCurrencyCode` | `CurrencyCode().fake()` | `faker.finance.currencyCode()` |
| `randomCurrencyName` | `CurrencyName().fake()` | `faker.finance.currencyName()` |
| `randomCurrencySymbol` | `CurrencySymbol().fake()` | `faker.finance.currencySymbol()` |
| `randomBitcoin` | custom: 26-35 char base58 string | `faker.finance.bitcoinAddress()` |

### Business (10)
| Variable | Rust | TypeScript |
|---|---|---|
| `randomCompanyName` | `CompanyName().fake()` | `faker.company.name()` |
| `randomCompanySuffix` | `CompanySuffix().fake()` | `faker.company.companySuffix()` |
| `randomBs` | `Bs().fake()` | `faker.company.buzzPhrase()` |
| `randomBsAdjective` | `BsAdj().fake()` (custom) | `faker.company.buzzAdjective()` |
| `randomBsBuzz` | `BsVerb().fake()` (custom) | `faker.company.buzzVerb()` |
| `randomBsNoun` | `BsNoun().fake()` (custom) | `faker.company.buzzNoun()` |
| `randomCatchPhrase` | `CatchPhrase().fake()` | `faker.company.catchPhrase()` |
| `randomCatchPhraseAdjective` | `CatchPhraseAdj().fake()` (custom) | `faker.company.catchPhraseAdjective()` |
| `randomCatchPhraseDescriptor` | `CatchPhraseDesc().fake()` (custom) | `faker.company.catchPhraseDescriptor()` |
| `randomCatchPhraseNoun` | `CatchPhraseNoun().fake()` (custom) | `faker.company.catchPhraseNoun()` |

### Database (4)
| Variable | Rust | TypeScript |
|---|---|---|
| `randomDatabaseColumn` | custom: random from [id, name, created_at, ...] | `faker.database.column()` |
| `randomDatabaseType` | custom: random from [varchar, int, ...] | `faker.database.type()` |
| `randomDatabaseCollation` | custom: random from [utf8_general_ci, ...] | `faker.database.collation()` |
| `randomDatabaseEngine` | custom: random from [InnoDB, MyISAM, ...] | `faker.database.engine()` |

### Dates (5)
| Variable | Rust | TypeScript |
|---|---|---|
| `randomDateFuture` | `chrono::Utc::now() + random days` | `faker.date.future().toISOString()` |
| `randomDatePast` | `chrono::Utc::now() - random days` | `faker.date.past().toISOString()` |
| `randomDateRecent` | `chrono::Utc::now() - random hours` | `faker.date.recent().toISOString()` |
| `randomWeekday` | custom: random from weekday list | `faker.date.weekday()` |
| `randomMonth` | custom: random from month list | `faker.date.month()` |

### Files and System (9)
| Variable | Rust | TypeScript |
|---|---|---|
| `randomFileName` | `FileName().fake()` | `faker.system.fileName()` |
| `randomFileType` | `MimeType().fake()` | `faker.system.fileType()` |
| `randomFileExt` | `FileExtension().fake()` | `faker.system.fileExt()` |
| `randomCommonFileName` | `FileName().fake()` | `faker.system.commonFileName()` |
| `randomCommonFileType` | custom: from [application, image, ...] | `faker.system.commonFileType()` |
| `randomCommonFileExt` | custom: from [pdf, jpg, png, ...] | `faker.system.commonFileExt()` |
| `randomFilePath` | `FilePath().fake()` | `faker.system.filePath()` |
| `randomDirectoryPath` | `DirPath().fake()` | `faker.system.directoryPath()` |
| `randomMimeType` | `MimeType().fake()` | `faker.system.mimeType()` |

### Commerce (6)
| Variable | Rust | TypeScript |
|---|---|---|
| `randomPrice` | `format!("{:.2}", rng.gen_range(1.0..1000.0))` | `faker.commerce.price()` |
| `randomProduct` | `ProductName().fake()` (single word) | `faker.commerce.product()` |
| `randomProductAdjective` | `ProductAdj().fake()` (custom) | `faker.commerce.productAdjective()` |
| `randomProductMaterial` | `ProductMaterial().fake()` (custom) | `faker.commerce.productMaterial()` |
| `randomProductName` | `ProductName().fake()` | `faker.commerce.productName()` |
| `randomDepartment` | `Department().fake()` (custom) | `faker.commerce.department()` |

### Hacker and Lorem (13)
| Variable | Rust | TypeScript |
|---|---|---|
| `randomNoun` | `Buzzword().fake()` | `faker.hacker.noun()` |
| `randomVerb` | `BsVerb().fake()` | `faker.hacker.verb()` |
| `randomIngverb` | `Ingverb().fake()` (custom) | `faker.hacker.ingverb()` |
| `randomAdjective` | `BsAdj().fake()` | `faker.hacker.adjective()` |
| `randomPhrase` | `CatchPhrase().fake()` | `faker.hacker.phrase()` |
| `randomLoremWord` | `Word().fake()` | `faker.lorem.word()` |
| `randomLoremWords` | `Words(3..5).fake()` | `faker.lorem.words()` |
| `randomLoremSentence` | `Sentence(5..10).fake()` | `faker.lorem.sentence()` |
| `randomLoremSentences` | `Sentences(2..4).fake()` | `faker.lorem.sentences()` |
| `randomLoremParagraph` | `Paragraph(3..5).fake()` | `faker.lorem.paragraph()` |
| `randomLoremParagraphs` | `Paragraphs(2..4).fake()` | `faker.lorem.paragraphs()` |
| `randomLoremText` | `Paragraph(3..5).fake()` | `faker.lorem.text()` |
| `randomLoremSlug` | `Words(3).fake().join("-")` | `faker.lorem.slug()` |
| `randomLoremLines` | `Sentences(2..4).fake()` (newline-joined) | `faker.lorem.lines()` |

---

## Testing

### Rust unit tests (`dynamic_vars.rs`)
- Loop test: every registered name returns `Some(...)`
- Unknown names return `None`
- `guid`/`randomUUID`: valid UUID v4 format
- `timestamp`: parseable as i64
- `isoTimestamp`: valid RFC 3339
- `randomInt`: within 0–1000
- `randomBoolean`: `"true"` or `"false"`
- Freshness: two calls produce different values (probabilistic, 10 iterations)

### Rust integration tests (`resolver.rs`)
- `{{$guid}}` resolves to UUID, not left as-is
- `{{$guid}}` + `{{$randomUUID}}` in same template → two different values
- `{{$unknownThing}}` left as-is, appears in `unresolved`
- `{{$guid}}` not shadowed by user variable `$guid`
- Mixed: `{{baseUrl}}/{{$randomUUID}}` — both parts resolve correctly
- `{{$timestamp}}` fresh per call

### TypeScript unit tests (`dynamic-vars.test.ts`)
- `isDynamicVar("guid")` → true
- `isDynamicVar("unknownThing")` → false
- `generateDynamicVar("guid")` → valid UUID string
- `generateDynamicVar("unknownThing")` → undefined
- `listDynamicVars()` → all 118 names

### TypeScript integration tests (`variable-context.test.ts`)
- `resolveWithContext("{{$guid}}", {})` → UUID, not `"{{$guid}}"`
- `resolveWithContext("{{$guid}}", { "$guid": "override" })` → still generated (no shadow)
- Mixed resolution: `"{{baseUrl}}/{{$randomInt}}"` with context

### URL bar tests (`url-variables.test.ts`)
- `{{$randomEmail}}` → token with `source: 'dynamic'`
- Popover shows `"Dynamic"` label + preview

---

## Error Handling

- **Unknown `$` variable**: left as `{{$unknownThing}}` in output, added to `unresolved`.
  No crash, no silent empty string. Identical to unknown regular variables.
- **`fake` crate safety**: generators don't panic under normal use. Direct match arm
  calls — no trait object indirection.
- **Frontend `@faker-js/faker` errors**: optional call `?.()` returns undefined,
  variable left unresolved.

---

## Files Changed

### New files
| File | Purpose |
|---|---|
| `crates/rocket-environment/src/dynamic_vars.rs` | Rust generator registry (118 match arms) |
| `src/lib/dynamic-vars.ts` | TypeScript generator registry (118 entries) |
| `src/lib/__tests__/dynamic-vars.test.ts` | TypeScript unit tests |

### Modified files
| File | Change |
|---|---|
| `crates/rocket-environment/Cargo.toml` | Add `fake`, `uuid`, `chrono`, `rand` deps |
| `crates/rocket-environment/src/lib.rs` | Add `pub mod dynamic_vars;` |
| `crates/rocket-environment/src/resolver.rs` | Add `$` prefix check before user lookup |
| `src/lib/variable-context.ts` | Update regex + `resolveWithContext` for `$` prefix |
| `src/lib/url-variables.ts` | Add `'dynamic'` to `VariableSource`, badge colour, token parsing |
| `src/components/request/VariableAwareInput.tsx` | Handle `'dynamic'` source in popover |
| `src/components/editor/MonacoWrapper.tsx` | Add `'dynamic'` decoration class |

### No new Tauri commands
Dynamic variables are resolved inline in the existing resolution pipeline.
No new IPC surface.

### No new crates
Everything lives inside `rocket-environment` (Rust) and `src/lib/` (TypeScript).

---

## Out of Scope

- **Parameterised dynamic variables** (e.g. `{{$timestamp|yyyyMMDD}}`) — Bruno has an
  open feature request for this; not yet supported. Can be added later by extending the
  regex and generator signature.
- **Custom dynamic variable registration** — users defining their own `$`-prefixed generators.
  This would be a scripting engine feature (SP3).
- **Autocomplete / intellisense** for `$` variables in Monaco editor — nice-to-have,
  separate spec.
