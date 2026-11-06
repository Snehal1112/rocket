import aiAssistant from '@/content/guide/ai-assistant.md?raw';
import authentication from '@/content/guide/authentication.md?raw';
import collectionRunner from '@/content/guide/collection-runner.md?raw';
import collectionsAndRequests from '@/content/guide/collections-and-requests.md?raw';
import contractLock from '@/content/guide/contract-lock.md?raw';
import environmentsAndVariables from '@/content/guide/environments-and-variables.md?raw';
import faq from '@/content/guide/faq.md?raw';
import flow from '@/content/guide/flow.md?raw';
import folderSettings from '@/content/guide/folder-settings.md?raw';
import gettingStarted from '@/content/guide/getting-started.md?raw';
import gitIntegration from '@/content/guide/git-integration.md?raw';
import importAndExport from '@/content/guide/import-and-export.md?raw';
import loadTesting from '@/content/guide/load-testing.md?raw';
import openapiSync from '@/content/guide/openapi-sync.md?raw';
import responseExamples from '@/content/guide/response-examples.md?raw';
import scriptingAndTesting from '@/content/guide/scripting-and-testing.md?raw';
import secretsAndVault from '@/content/guide/secrets-and-vault.md?raw';
import securityAndCompliance from '@/content/guide/security-and-compliance.md?raw';
import settings from '@/content/guide/settings.md?raw';

// Keyed by the same slug used in sidebar.ts and the route path.
export const content: Record<string, string> = {
  'getting-started': gettingStarted,
  'collections-and-requests': collectionsAndRequests,
  'environments-and-variables': environmentsAndVariables,
  authentication,
  'scripting-and-testing': scriptingAndTesting,
  'collection-runner': collectionRunner,
  'load-testing': loadTesting,
  flow,
  'response-examples': responseExamples,
  'git-integration': gitIntegration,
  'import-and-export': importAndExport,
  'folder-settings': folderSettings,
  'openapi-sync': openapiSync,
  'contract-lock': contractLock,
  'ai-assistant': aiAssistant,
  'secrets-and-vault': secretsAndVault,
  'security-and-compliance': securityAndCompliance,
  settings,
  faq,
};
