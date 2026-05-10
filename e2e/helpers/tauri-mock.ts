/**
 * Tauri IPC mock for Playwright E2E tests.
 *
 * Injected via page.addInitScript before React initialises.
 * Implements window.__TAURI_INTERNALS__ (the actual surface used by
 * @tauri-apps/api/core) plus window.__TAURI_OS_PLUGIN_INTERNALS__ for
 * the OS plugin's synchronous type lookup.
 *
 * State lives in window.__e2eMockState so tests can inspect/mutate it
 * via page.evaluate.
 */
export const TAURI_MOCK_SCRIPT = `
(function() {
  // ── Callback registry (used by Channel / event system) ───────────────────
  const callbacks = {};
  let nextCallbackId = 1;

  function transformCallback(fn, once) {
    const id = nextCallbackId++;
    callbacks[id] = { fn, once: !!once };
    return id;
  }
  function unregisterCallback(id) { delete callbacks[id]; }

  // ── In-memory mock state ──────────────────────────────────────────────────
  const state = window.__e2eMockState = {
    contracts: [],
    nextContractId: 1,
  };

  function ulid() {
    return 'E2E' + String(state.nextContractId++).padStart(22, '0');
  }
  function todayISO() { return new Date().toISOString().slice(0, 10); }

  // ── Fixture data ──────────────────────────────────────────────────────────
  const workspace = { id: 'e2e-workspace', name: 'E2E Test Workspace', path: '/tmp/rocket-e2e', pinned: true, description: null };
  const collectionSummary = { uid: 'e2e-test-collection-uid', name: 'TestCollection', itemCount: 2, requestCount: 2, refType: 'embedded' };
  const fullCollection = {
    ...collectionSummary,
    root: {
      uid: 'root-folder',
      name: 'root',
      items: [
        { type: 'request', uid: 'get-payments-uid', name: 'Get Payments', method: 'GET', url: '{{baseUrl}}/payments', fileName: 'get-payments.yml' },
        { type: 'request', uid: 'post-payments-uid', name: 'Create Payment', method: 'POST', url: '{{baseUrl}}/payments', fileName: 'post-payments.yml' },
      ],
    },
    settings: { auth: { authType: 'none' }, headers: [], variables: [] },
  };

  // ── Core invoke handler ───────────────────────────────────────────────────
  async function invoke(command, args, _options) {
    args = args || {};

    // Tauri v2 plugin commands use "plugin:name|method" format
    if (command.startsWith('plugin:')) {
      const pluginName = command.split(':')[1].split('|')[0];
      switch (pluginName) {
        case 'os':
        case 'notification':
        case 'updater':
        case 'opener':
        case 'dialog':
        case 'fs':
          return null;
        case 'app':
          return '0.0.0-e2e';
        default:
          console.warn('[tauri-mock] unhandled plugin command:', command);
          return null;
      }
    }

    switch (command) {
      // Workspace
      case 'list_workspaces':          return [workspace];
      case 'get_active_workspace':     return workspace;
      case 'get_workspace_config':     return { name: workspace.name, collections: [], environments: [] };
      case 'get_multi_workspace_mode': return false;
      case 'switch_workspace':         return workspace;
      case 'get_app_data_dir':         return '/tmp/rocket-e2e';

      // Collections
      case 'list_collections':
      case 'get_collection_summaries': return [collectionSummary];
      case 'get_collection':           return fullCollection;
      case 'scan_collections_in_path': return [collectionSummary];
      case 'detect_cloned_structure':  return null;
      case 'get_folder_chain_variables':
      case 'get_folder_variables':
      case 'get_request_variables':    return [];

      // Environments
      case 'list_environments':
      case 'list_global_environments': return [];
      case 'get_global_environment_name': return '';
      case 'get_environment':          return null;
      case 'get_process_env_vars':     return {};
      case 'get_global_environment':   return null;

      // File watcher
      case 'watch_collections':
      case 'stop_watching':            return null;

      // UI state / misc
      case 'load_ui_state':            return null;
      case 'save_ui_state':            return null;
      case 'get_app_data_dir':         return '/tmp/rocket-e2e';

      // History / templates / cookies
      case 'list_history':             return [];
      case 'list_templates':           return [];
      case 'get_cookies':              return {};
      case 'clear_history':            return null;

      // Git
      case 'git_is_repo':              return false;

      // Audit
      case 'list_audit_events':
      case 'list_audit_events_range':  return [];
      case 'get_compliance_profile':   return { level: 'none' };

      // OAuth
      case 'oauth2_get_token':
      case 'oauth2_decode_jwt':        return null;

      // ── Contract CRUD ─────────────────────────────────────────────────────

      case 'list_contracts':           return state.contracts.slice();

      case 'attach_contract': {
        const inp = args.input || {};
        const c = {
          id: ulid(),
          title: inp.title || 'Untitled',
          provider: inp.provider || { id: 'provider', name: 'Provider', kind: 'team' },
          consumers: inp.consumers || [],
          project: '',
          version: inp.version || '1.0.0',
          status: inp.publishImmediately ? 'active' : 'draft',
          effectiveDate: inp.effectiveDate || todayISO(),
          expiryDate: inp.expiryDate || null,
          documentPaths: [],
          enforcementMode: 'informational',
          scope: inp.scope || { type: 'collection' },
          policy: inp.policy || { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null },
          driftCount: 0, breachCount: 0, endpointCount: 2,
          createdBy: null,
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
        };
        state.contracts.push(c);
        return c;
      }

      case 'get_contract':
        return state.contracts.find(c => c.id === args.contractId) || null;

      case 'delete_contract': {
        const i = state.contracts.findIndex(c => c.id === args.contractId);
        if (i >= 0) state.contracts.splice(i, 1);
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
        if (!src) return null;
        const copy = { ...src, id: ulid(), title: src.title + ' (copy)', status: 'draft', updatedAt: new Date().toISOString() };
        state.contracts.push(copy);
        return copy;
      }

      case 'recompute_drift': {
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
        return state.contracts.map(c => ({ contractId: c.id, status: c.status, driftCount: c.driftCount, breachCount: c.breachCount }));
      }

      case 'get_contract_summary':
        return state.contracts.map(c => ({ id: c.id, title: c.title, status: c.status, driftCount: c.driftCount, breachCount: c.breachCount }));

      case 'get_contract_changelog':
        return { contractId: args.contractId, entries: [] };

      case 'export_contract_openapi':
        return 'openapi: 3.0.0\ninfo:\n  title: Mock\n  version: 1.0.0\n';

      default:
        console.warn('[tauri-mock] unhandled command:', command);
        return null;
    }
  }

  // ── Event mock ────────────────────────────────────────────────────────────
  const listeners = {};
  function listen(event, handler) {
    if (!listeners[event]) listeners[event] = [];
    listeners[event].push(handler);
    return Promise.resolve(() => {
      const arr = listeners[event] || [];
      const i = arr.indexOf(handler);
      if (i >= 0) arr.splice(i, 1);
    });
  }
  window.__e2eEmitEvent = (event, payload) => {
    (listeners[event] || []).forEach(h => h({ event, payload }));
  };

  // ── OS plugin internals (read synchronously by @tauri-apps/plugin-os) ─────
  window.__TAURI_OS_PLUGIN_INTERNALS__ = { os_type: 'macos' };

  // ── window.__TAURI_INTERNALS__ (used by @tauri-apps/api/core) ────────────
  window.__TAURI_INTERNALS__ = {
    invoke,
    transformCallback,
    unregisterCallback,
    unregisterListener: () => {},
    convertFileSrc: (path) => path,
    metadata: { currentWindow: { label: 'main' }, windows: [{ label: 'main' }] },
  };

  // ── window.__TAURI__ (withGlobalTauri compatibility) ─────────────────────
  window.__TAURI__ = {
    core: { invoke, transformCallback, Channel: class {} },
    event: { listen, emit: () => Promise.resolve(), once: () => Promise.resolve(() => {}) },
    path: { appDataDir: () => Promise.resolve('/tmp/rocket-e2e') },
    dialog: { open: () => Promise.resolve(null), save: () => Promise.resolve(null) },
    fs: { readTextFile: () => Promise.resolve(''), writeTextFile: () => Promise.resolve() },
  };
})();
`;
