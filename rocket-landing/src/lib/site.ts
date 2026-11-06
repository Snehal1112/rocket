// Outbound links for the landing page, pointed at the real project repository.
export const links = {
  github: 'https://github.com/Snehal1112/rocket',
  docs: 'https://docs.rocket.numericlabs.io',
  license: 'https://github.com/Snehal1112/rocket/blob/main/LICENSE',
  benchmarks: 'https://github.com/Snehal1112/rocket/blob/main/docs/engineering/benchmarks.md',
  contactEmail: 'snehal.dangroshiya@numericlabs.io',
} as const;

// Anchor targets used by the nav and the in-page links.
export const sections = {
  features: 'features',
  assistant: 'ai-assistant',
  flow: 'flow',
  benchmarks: 'benchmarks',
  security: 'security',
  openapi: 'openapi-sync',
  contracts: 'contracts',
  download: 'download',
} as const;

export const navLinks = [
  { href: `#${sections.features}`, label: 'Features' },
  { href: `#${sections.assistant}`, label: 'AI Assistant' },
  { href: `#${sections.flow}`, label: 'Flow' },
  { href: `#${sections.benchmarks}`, label: 'Benchmarks' },
  { href: `#${sections.security}`, label: 'Security' },
  { href: `#${sections.openapi}`, label: 'OpenAPI Sync' },
  { href: `#${sections.contracts}`, label: 'Contracts' },
];

// Every measured figure on this page comes from the same run.
export const benchmarkMeta = {
  measuredOn: '2026-10-21',
  versions: 'Rocket 2.1.1 · Bruno 4.2.0 · Postman 11.71.7',
  machine: 'the same Linux machine',
};
