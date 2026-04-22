import { faker } from '@faker-js/faker';

/**
 * Registry of dynamic variables supported by Rocket.
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
  randomProtocol: () => faker.helpers.arrayElement(['http', 'https']),
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
  randomPhoneNumberExt: () =>
    `${faker.phone.number()} ext. ${faker.number.int({ min: 100, max: 999 })}`,

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
  randomCreditCardMask: () => `XXXX-XXXX-XXXX-${faker.number.int({ min: 1000, max: 9999 })}`,
  randomBankAccountBic: () => faker.finance.bic(),
  randomBankAccountIban: () => faker.finance.iban(),
  randomTransactionType: () => faker.finance.transactionType(),
  randomCurrencyCode: () => faker.finance.currencyCode(),
  randomCurrencyName: () => faker.finance.currencyName(),
  randomCurrencySymbol: () => faker.finance.currencySymbol(),
  randomBitcoin: () => faker.finance.bitcoinAddress(),

  // ── Business ──
  randomCompanyName: () => faker.company.name(),
  randomCompanySuffix: () => faker.helpers.arrayElement(['Inc', 'LLC', 'Group', 'Ltd', 'Co']),
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
