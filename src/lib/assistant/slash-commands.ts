import type { SlashCommandItem } from './types';

/** Rocket's own prompt templates. Choosing one puts the template text in the editor. */
export const SLASH_COMMANDS: readonly SlashCommandItem[] = [
  {
    name: 'explain',
    description: 'Explain what a request and its scripts do',
    template: 'Explain what this request does, including its scripts and tests: ',
  },
  {
    name: 'tests',
    description: 'Write tests for a request',
    template:
      'Write tests for this request that check the status code, the important response fields and the response time: ',
  },
  {
    name: 'fix',
    description: 'Find and fix a failing script or test',
    template: 'This request or one of its scripts fails. Find the cause and propose a fix: ',
  },
  {
    name: 'scaffold',
    description: 'Create requests and folders',
    template: 'Create folders and requests for the following API: ',
  },
  {
    name: 'doc',
    description: 'Write documentation for a request',
    template:
      'Write short Markdown documentation for this request: what it does, its parameters and an example response: ',
  },
];

/** The commands whose name starts with `query`, ignoring case. */
export function filterSlashCommands(query: string): SlashCommandItem[] {
  const prefix = query.trim().toLowerCase();
  return SLASH_COMMANDS.filter((command) => command.name.startsWith(prefix));
}
