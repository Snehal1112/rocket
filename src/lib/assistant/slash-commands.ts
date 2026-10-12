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
      'Write Markdown documentation for this request with the sections Purpose, Authentication, Parameters, Request body, Responses and errors, and Example. Use only what the request, its folder settings and its tests show, and write "Not documented" where you do not know: ',
  },
  {
    name: 'review',
    description: 'Review a request for mistakes and say how to fix them',
    template:
      'Review this request for mistakes. Check its URL, auth, headers, parameters, body, scripts and tests against its docs and its folder settings, explain what is wrong and why, and propose the smallest fixes: ',
  },
  {
    name: 'howto',
    description: 'Explain how to call an endpoint correctly',
    template:
      'Explain step by step how to call this endpoint correctly: which variables, auth and headers I need, and what to check if it fails: ',
  },
];

/** The commands whose name starts with `query`, ignoring case. */
export function filterSlashCommands(query: string): SlashCommandItem[] {
  const prefix = query.trim().toLowerCase();
  return SLASH_COMMANDS.filter((command) => command.name.startsWith(prefix));
}
