export interface SidebarItem {
  title: string;
  slug: string;
}

export interface SidebarGroup {
  title: string;
  items: SidebarItem[];
}

export const sidebar: SidebarGroup[] = [
  {
    title: 'Start here',
    items: [
      { title: 'Getting started', slug: 'getting-started' },
      { title: 'Collections & requests', slug: 'collections-and-requests' },
      { title: 'Environments & variables', slug: 'environments-and-variables' },
      { title: 'Authentication', slug: 'authentication' },
    ],
  },
  {
    title: 'Build and test',
    items: [
      { title: 'Scripting & testing', slug: 'scripting-and-testing' },
      { title: 'Collection Runner', slug: 'collection-runner' },
      { title: 'Load testing', slug: 'load-testing' },
      { title: 'Flow — visual workflows', slug: 'flow' },
      { title: 'Response examples', slug: 'response-examples' },
    ],
  },
  {
    title: 'Work with your team',
    items: [
      { title: 'Git integration', slug: 'git-integration' },
      { title: 'Import & export', slug: 'import-and-export' },
      { title: 'Folder settings', slug: 'folder-settings' },
      { title: 'OpenAPI Sync', slug: 'openapi-sync' },
      { title: 'Contract Lock', slug: 'contract-lock' },
    ],
  },
  {
    title: 'The AI Assistant',
    items: [{ title: 'AI Assistant', slug: 'ai-assistant' }],
  },
  {
    title: 'Security',
    items: [
      { title: 'Secrets & vault', slug: 'secrets-and-vault' },
      { title: 'Security & compliance', slug: 'security-and-compliance' },
    ],
  },
  {
    title: 'Reference',
    items: [
      { title: 'Settings', slug: 'settings' },
      { title: 'FAQ & troubleshooting', slug: 'faq' },
    ],
  },
];

export const sidebarItems: SidebarItem[] = sidebar.flatMap((group) => group.items);

export const links = {
  github: 'https://github.com/Snehal1112/rocket',
  site: 'https://web.rocket.numericlabs.io',
} as const;
