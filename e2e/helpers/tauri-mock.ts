/**
 * Tauri IPC mock for Playwright E2E tests.
 *
 * Injected via page.addInitScript before React initialises.
 * Provides enough of the Tauri API surface to let the app boot and render
 * the contract feature without a real Tauri desktop runtime.
 *
 * State is held in-process (window.__e2eMockState) so tests can
 * inspect and mutate it via page.evaluate.
 */
export const TAURI_MOCK_SCRIPT = `
(function() {
  // ── In-memory state ──────────────────────────────────────────────────────
  const WORKSPACE_ID  = 'e2e-workspace';
  const WORKSPACE_PATH = '/tmp/rocket-e2e-workspace';
  const COLLECTION_ROOT = WORKSPACE_PATH + '/collections/TestCollection';

  const state = window.__e2eMockState = {
    contracts: [],     // Array of contract objects for TestCollection
    nextContractId: 1,
  };

  function ulid() {
    return 'E2E' + String(state.nextContractId++).padStart(22, '0');
  }

  function todayISO() {
    return new Date().toISOString().slice(0, 10);
  }

  // ── Workspace data ────────────────────────────────────────────────────────
  const workspace = {
    id: WORKSPACE_ID,
    name: 'E2E Test Workspace',
    path: WORKSPACE_PATH,
    pinned: true,
    description: null,
    multiWorkspaceMode: false,
  };

  const collectionSummary = {
    uid:  'e2e-test-collection-0000-0000-0000',
    name: 'TestCollection',
    itemCount: 2,
    requestCount: 2,
    description: 'E2E test collection',
    refType: 'embedded',
  };

  const fullCollection = {
    ...collectionSummary,
    root: {
      uid: 'e2e-folder-root',
      name: 'root',
      items: [
        {
          type: 'request',
          uid: 'e2e-get-payments-0000-0000-0001',
          name: 'Get Payments',
          method: 'GET',
          url: '{{baseUrl}}/payments',
          fileName: 'get-payments.yml',
        },
        {
          type: 'request',
          uid: 'e2e-post-payments-0000-0000-0002',
          name: 'Create Payment',
          method: 'POST',
          url: '{{baseUrl}}/payments',
          fileName: 'post-payments.yml',
        },
      ],
    },
    settings: { auth: { authType: 'none' }, headers: [], variables: [] },
  };

  // ── Core invoke handler ───────────────────────────────────────────────────
  async function invoke(command, args) {
    args = args || {};

    switch (command) {
      // Workspace
      case 'list_workspaces':       return [workspace];
      case 'get_active_workspace':  return workspace;
      case 'get_workspace_config':  return { name: workspace.name, collections: [], environments: [] };
      case 'get_multi_workspace_mode': return false;
      case 'switch_workspace':      return workspace;
      case 'get_app_data_dir':      return WORKSPACE_PATH;

      // Collections
      case 'list_collections':
      case 'get_collection_summaries':
        return [collectionSummary];
      case 'get_collection':
        return fullCollection;
      case 'scan_collections_in_path':
        return [collectionSummary];
      case 'detect_cloned_structure':
        return null;

      // Environments
      case 'list_environments':
      case 'list_global_environments':
        return [];
      case 'get_global_environment_name':
        return '';
      case 'get_environment':
        return null;
      case 'get_process_env_vars':
        return {};

      // File watcher
      case 'watch_collections':
      case 'stop_watching':
        return null;

      // UI state
      case 'load_ui_state':
        return null;
      case 'save_ui_state':
        return null;

      // History / templates / cookies
      case 'list_history':    return [];
      case 'list_templates':  return [];
      case 'get_cookies':     return {};

      // Git
      case 'git_is_repo':     return false;

      // Audit
      case 'list_audit_events':
      case 'list_audit_events_range':
        return [];
      case 'get_compliance_profile':
        return { level: 'none' };

      // OAuth
      case 'oauth2_get_token':
      case 'oauth2_decode_jwt':
        return null;

      // ── Contract commands ─────────────────────────────────────────────────

      case 'list_contracts':
        return state.contracts.slice();

      case 'attach_contract': {
        const input = args.input || {};
        const contract = {
          id: ulid(),
          title: input.title || 'Untitled',
          provider: input.provider || { id: 'provider', name: 'Provider', kind: 'team' },
          consumers: input.consumers || [],
          project: '',
          version: input.version || '1.0.0',
          status: input.publishImmediately ? 'active' : 'draft',
          effectiveDate: input.effectiveDate || todayISO(),
          expiryDate: input.expiryDate || null,
          documentPaths: [],
          enforcementMode: 'informational',
          scope: input.scope || { type: 'collection' },
          policy: input.policy || { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null },
          driftCount: 0,
          breachCount: 0,
          endpointCount: 2,
          createdBy: null,
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
        };
        state.contracts.push(contract);
        return contract;
      }

      case 'get_contract': {
        const c = state.contracts.find(c => c.id === args.contractId);
        if (!c) throw new Error('Contract not found: ' + args.contractId);
        return c;
      }

      case 'delete_contract': {
        const idx = state.contracts.findIndex(c => c.id === args.contractId);
        if (idx >= 0) state.contracts.splice(idx, 1);
        return null;
      }

      case 'publish_contract': {
        const c = state.contracts.find(c => c.id === args.contractId);
        if (c) { c.status = 'active'; c.updatedAt = new Date().toISOString(); }
        return c;
      }

      case 'pause_contract': {
        const c = state.contracts.find(c => c.id === args.contractId);
        if (c) { c.status = 'paused'; c.updatedAt = new Date().toISOString(); }
        return c;
      }

      case 'resume_contract': {
        const c = state.contracts.find(c => c.id === args.contractId);
        if (c) { c.status = 'active'; c.updatedAt = new Date().toISOString(); }
        return c;
      }

      case 'renew_contract': {
        const c = state.contracts.find(c => c.id === args.contractId);
        if (c) { c.status = 'active'; c.expiryDate = args.newExpiresAt || null; c.updatedAt = new Date().toISOString(); }
        return c;
      }

      case 'send_for_review': {
        const c = state.contracts.find(c => c.id === args.contractId);
        if (c) { c.status = 'in_review'; c.updatedAt = new Date().toISOString(); }
        return c;
      }

      case 'approve_contract': {
        const c = state.contracts.find(c => c.id === args.contractId);
        if (c) { c.status = 'active'; c.updatedAt = new Date().toISOString(); }
        return c;
      }

      case 'reject_contract': {
        const c = state.contracts.find(c => c.id === args.contractId);
        if (c) { c.status = 'draft'; c.updatedAt = new Date().toISOString(); }
        return c;
      }

      case 'duplicate_contract': {
        const src = state.contracts.find(c => c.id === args.contractId);
        if (!src) throw new Error('Contract not found');
        const copy = { ...src, id: ulid(), title: src.title + ' (copy)', status: 'draft', updatedAt: new Date().toISOString() };
        state.contracts.push(copy);
        return copy;
      }

      case 'recompute_drift': {
        // If __e2eDriftMode is set, update contract statuses accordingly
        const mode = window.__e2eDriftMode;
        if (mode === 'drift' || mode === 'breach') {
          state.contracts.forEach(c => {
            if (c.status === 'active') {
              c.status = mode;
              c.driftCount = 1;
              c.breachCount = mode === 'breach' ? 1 : 0;
              c.updatedAt = new Date().toISOString();
            }
          });
        }
        return state.contracts.map(c => ({
          contractId: c.id,
          status: c.status,
          driftCount: c.driftCount,
          breachCount: c.breachCount,
        }));
      }

      case 'get_contract_summary':
        return state.contracts.map(c => ({
          id: c.id,
          title: c.title,
          status: c.status,
          driftCount: c.driftCount,
          breachCount: c.breachCount,
        }));

      case 'get_contract_changelog':
        return { contractId: args.contractId, entries: [] };

      default:
        console.warn('[tauri-mock] unhandled command:', command, args);
        return null;
    }
  }

  // ── Tauri event mock ──────────────────────────────────────────────────────
  const listeners = {};
  function listen(event, handler) {
    if (!listeners[event]) listeners[event] = [];
    listeners[event].push(handler);
    return Promise.resolve(() => {
      const idx = (listeners[event] || []).indexOf(handler);
      if (idx >= 0) listeners[event].splice(idx, 1);
    });
  }

  window.__e2eEmitEvent = function(event, payload) {
    (listeners[event] || []).forEach(h => h({ event, payload }));
  };

  // ── Inject window.__TAURI__ ───────────────────────────────────────────────
  window.__TAURI__ = {
    core:  { invoke },
    event: { listen, emit: () => Promise.resolve(), once: () => Promise.resolve(() => {}) },
    path:  { appDataDir: () => Promise.resolve('/tmp/rocket-e2e') },
    dialog: {
      open:  () => Promise.resolve(null),
      save:  () => Promise.resolve(null),
    },
    fs: {
      readTextFile:  () => Promise.resolve(''),
      writeTextFile: () => Promise.resolve(),
    },
  };

  // Also mock the isTauri check some libraries use
  window.__TAURI_INTERNALS__ = window.__TAURI__;
})();
`;
