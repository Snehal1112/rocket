"use strict";

// Everything lives inside this IIFE so the captured ops table stays a closure
// variable. The user script runs in a separate execute_script() call outside
// this closure, so it has no syntactic way to reach `__ops`.
(function () {
  // Captured once, before the Deno global is removed at the bottom of this
  // IIFE. Every wrapper below calls through this private reference rather than
  // resolving a global identifier at call time.
  const __ops = Deno.core.ops;

  // ── console ────────────────────────────────────────────────────────────────
  function _fmt(v) {
    if (v === null || v === undefined) return String(v);
    if (typeof v === 'object' || Array.isArray(v)) {
      try { return JSON.stringify(v); } catch { return String(v); }
    }
    return String(v);
  }
  const console = {
    log:   (...args) => __ops.op_console_log(args.map(_fmt).join(" ")),
    warn:  (...args) => __ops.op_console_warn(args.map(_fmt).join(" ")),
    error: (...args) => __ops.op_console_error(args.map(_fmt).join(" ")),
  };
  globalThis.console = console;

  // ── read-your-writes overlay ────────────────────────────────────────────────
  // Ops read a snapshot taken before the script ran. These maps remember this
  // script's own sets and deletes so later reads in the same script agree.
  const _GONE = Symbol('gone');
  const _ov = { runtime: new Map(), env: new Map(), collection: new Map(), global: new Map() };

  function _ovRead(scope, key, base) {
    const m = _ov[scope];
    if (m.has(key)) { const v = m.get(key); return v === _GONE ? "" : v; }
    return base(key);
  }
  function _ovHas(scope, key, base) {
    const m = _ov[scope];
    if (m.has(key)) return m.get(key) !== _GONE;
    return base(key);
  }
  function _ovAll(scope, base) {
    const out = base();
    for (const [k, v] of _ov[scope]) {
      if (v === _GONE) delete out[k]; else out[k] = v;
    }
    return out;
  }
  function _ovDeleteAll(scope, base) {
    for (const k of Object.keys(_ovAll(scope, base))) _ov[scope].set(k, _GONE);
  }

  // ── host calls ──────────────────────────────────────────────────────────────
  // Async calls such as rok.sendRequest reach the app through async ops. The ops
  // take and return JSON with snake_case keys (HostRequest and HostResponse).
  function _hostResponse(r) {
    const headers = {};
    for (const [k, v] of r.headers) headers[String(k).toLowerCase()] = v;
    let data = r.body;
    try { data = JSON.parse(r.body); } catch (_e) { /* Not JSON, keep the text. */ }
    return { status: r.status, statusText: r.status_text, headers, data, responseTime: r.response_time_ms };
  }

  function _sendOptions(options) {
    if (!options || typeof options !== 'object') {
      throw new TypeError('rok.sendRequest: options must be an object');
    }
    if (typeof options.url !== 'string' || options.url === '') {
      throw new TypeError('rok.sendRequest: url is required');
    }
    const headers = Object.entries(options.headers || {}).map(([k, v]) => [String(k), String(v)]);
    let body = null;
    let bodyIsJson = false;
    if (options.data !== undefined && options.data !== null) {
      if (typeof options.data === 'string') {
        body = options.data;
      } else {
        body = JSON.stringify(options.data);
        bodyIsJson = true;
      }
    }
    const timeout = typeof options.timeout === 'number' && options.timeout > 0
      ? Math.min(Math.floor(options.timeout), 300000)
      : 30000;
    return JSON.stringify({
      method: String(options.method || 'GET').toUpperCase(),
      url: options.url,
      headers,
      body,
      body_is_json: bodyIsJson,
      timeout_ms: timeout,
    });
  }

  // ── rok ─────────────────────────────────────────────────────────────────────
  globalThis.rok = {
    getVar:     (key) => _ovRead('runtime', key, (k) => __ops.op_rok_get_var(k)),
    setVar:     (key, value) => { __ops.op_rok_set_var(key, JSON.stringify(value)); _ov.runtime.set(key, value); },
    hasVar:     (key) => _ovHas('runtime', key, (k) => __ops.op_rok_has_var(k)),
    getAllVars: () => _ovAll('runtime', () => JSON.parse(__ops.op_rok_get_all_vars())),
    deleteVar:  (key) => { __ops.op_rok_delete_var(key); _ov.runtime.set(key, _GONE); },
    deleteAllVars: () => {
      _ovDeleteAll('runtime', () => JSON.parse(__ops.op_rok_get_all_vars()));
      __ops.op_rok_delete_all_vars();
    },

    getEnvVar:  (key) => _ovRead('env', key, (k) => __ops.op_rok_get_env_var(k)),
    setEnvVar:  (key, value, opts) => {
      __ops.op_rok_set_env_var(key, JSON.stringify(value), !!(opts && opts.persist));
      _ov.env.set(key, value);
    },
    hasEnvVar:  (key) => _ovHas('env', key, (k) => __ops.op_rok_has_env_var(k)),
    getAllEnvVars: () => _ovAll('env', () => JSON.parse(__ops.op_rok_get_all_env_vars())),
    deleteEnvVar: (key) => { __ops.op_rok_delete_env_var(key); _ov.env.set(key, _GONE); },
    deleteAllEnvVars: () => {
      _ovDeleteAll('env', () => JSON.parse(__ops.op_rok_get_all_env_vars()));
      __ops.op_rok_delete_all_env_vars();
    },

    getCollectionVar: (key) => _ovRead('collection', key, (k) => __ops.op_rok_get_collection_var(k)),
    setCollectionVar: (key, value) => {
      __ops.op_rok_set_collection_var(key, JSON.stringify(value));
      _ov.collection.set(key, value);
    },
    hasCollectionVar: (key) => _ovHas('collection', key, (k) => __ops.op_rok_has_collection_var(k)),
    deleteCollectionVar: (key) => { __ops.op_rok_delete_collection_var(key); _ov.collection.set(key, _GONE); },
    // Known limit: the collection scope has no read-all op, so only keys this
    // script touched are marked gone; untouched snapshot keys still read normally.
    deleteAllCollectionVars: () => {
      for (const k of Array.from(_ov.collection.keys())) _ov.collection.set(k, _GONE);
      __ops.op_rok_delete_all_collection_vars();
    },

    getGlobalEnvVar: (key) => _ovRead('global', key, (k) => __ops.op_rok_get_global_env_var(k)),
    setGlobalEnvVar: (key, value) => {
      __ops.op_rok_set_global_env_var(key, JSON.stringify(value));
      _ov.global.set(key, value);
    },
    hasGlobalEnvVar: (key) => _ovHas('global', key, (k) => __ops.op_rok_has_global_env_var(k)),
    getAllGlobalEnvVars: () => _ovAll('global', () => JSON.parse(__ops.op_rok_get_all_global_env_vars())),
    deleteGlobalEnvVar: (key) => { __ops.op_rok_delete_global_env_var(key); _ov.global.set(key, _GONE); },
    deleteAllGlobalEnvVars: () => {
      _ovDeleteAll('global', () => JSON.parse(__ops.op_rok_get_all_global_env_vars()));
      __ops.op_rok_delete_all_global_env_vars();
    },

    getEnvName:        ()           => __ops.op_rok_get_env_name(),
    getCollectionName: ()           => __ops.op_rok_get_collection_name(),
    getTestResults:      () => JSON.parse(__ops.op_rok_get_test_results()),
    getAssertionResults: () => JSON.parse(__ops.op_rok_get_assertion_results()),
    isSafeMode:        ()           => __ops.op_rok_is_safe_mode(),
    cwd:               ()           => __ops.op_rok_cwd(),
    getSecretVar:      (key)        => __ops.op_rok_get_secret_var(key),
    getFolderVar:      (key)        => __ops.op_rok_get_folder_var(key),
    interpolate:       (template)   => __ops.op_rok_interpolate(template),
    getRequestVar:       (key) => __ops.op_rok_get_request_var(key),
    getProcessEnv:       (key) => (__ops.op_rok_has_process_env(key) ? __ops.op_rok_get_process_env(key) : undefined),
    setNextRequest:      (name) => __ops.op_rok_set_next_request(name == null ? "" : String(name)),
    sendRequest: async (options) => _hostResponse(JSON.parse(await __ops.op_rok_send_request(_sendOptions(options)))),
    runner: {
      setNextRequest: (name)  => __ops.op_rok_set_next_request(name == null ? "" : String(name)),
      skipRequest:    ()      => __ops.op_rok_skip_request(),
      stopExecution:   ()      => __ops.op_rok_stop_execution(),
      iterationIndex:  0,
      totalIterations: 1,
    },
  };

  // ── Developer-mode globals ──────────────────────────────────────────────────
  // __dirname is the collection root. The executing script's own path is not
  // known here, so __filename stays undefined. Local modules loaded through
  // require() get their own __dirname and __filename from their wrapper.
  if (!__ops.op_rok_is_safe_mode()) {
    try { globalThis.__dirname = __ops.op_rok_cwd(); } catch (_e) { /* No collection directory. */ }
    globalThis.__filename = undefined;
  }

  // ── HeaderList (PropertyList) ────────────────────────────────────────────────
  // One implementation backs req.headerList (writable) and res.headerList
  // (read-only). `items` is a private array of { key, value, disabled? } objects
  // and is only ever handed out as clones. `writer` is null for a read-only
  // list, otherwise { set(key, value), del(key) } calling the req mutation ops.
  // A write calls its op first, so a phase that forbids the write throws before
  // the local copy changes. Key lookups are case-insensitive.
  function _hkey(k) { return String(k).toLowerCase(); }

  function _parseHeaderLine(line) {
    const i = line.indexOf(':');
    if (i < 0) return { key: line.trim(), value: '' };
    return { key: line.slice(0, i).trim(), value: line.slice(i + 1).trim() };
  }

  // Accepts (name, value), a "Key: Value" string, or a { key, value } object.
  function _toHeader(a, b) {
    if (typeof a === 'string') {
      return b === undefined ? _parseHeaderLine(a) : { key: a, value: String(b) };
    }
    if (a && typeof a === 'object' && a.key !== undefined) {
      return { key: String(a.key), value: a.value === undefined ? '' : String(a.value) };
    }
    return null;
  }

  // Accepts a PropertyList, an array of header-likes, or a multi-line string.
  function _headerItems(src) {
    if (typeof src === 'string') {
      return src.split(/\r?\n/).filter((l) => l.trim() !== '').map(_parseHeaderLine);
    }
    if (src && typeof src.all === 'function') return src.all();
    if (Array.isArray(src)) return src.map((x) => _toHeader(x)).filter(Boolean);
    return [];
  }

  function _makeHeaderList(items, writer) {
    const clone = (h) => Object.assign({}, h);
    const snap = () => items.map(clone);
    const idx = (k) => items.findIndex((h) => _hkey(h.key) === _hkey(k));
    const needWriter = () => {
      if (!writer) throw new Error('HeaderList is read-only');
    };

    function put(h) {
      writer.set(h.key, h.value);
      const i = idx(h.key);
      if (i < 0) {
        items.push({ key: h.key, value: h.value });
        return true;
      }
      items[i] = { key: h.key, value: h.value };
      return false;
    }

    function dropKey(key) {
      writer.del(key);
      for (let i = items.length - 1; i >= 0; i--) {
        if (_hkey(items[i].key) === _hkey(key)) items.splice(i, 1);
      }
    }

    const list = {
      // read
      get: (name) => { const i = idx(name); return i < 0 ? undefined : items[i].value; },
      one: (name) => { const i = idx(name); return i < 0 ? undefined : clone(items[i]); },
      all: snap,
      count: () => items.length,
      // search
      has: (a, b) => {
        const key = (a && typeof a === 'object') ? a.key : a;
        if (idx(key) < 0) return false;
        if (b === undefined) return true;
        return items.some((h) => _hkey(h.key) === _hkey(key) && h.value === b);
      },
      find: (fn, ctx) => snap().find((h, i) => fn.call(ctx, h, i)),
      filter: (fn, ctx) => snap().filter((h, i) => fn.call(ctx, h, i)),
      indexOf: (item) => {
        if (typeof item === 'string') return idx(item);
        if (!item || item.key === undefined) return -1;
        return items.findIndex((h) => _hkey(h.key) === _hkey(item.key)
          && (item.value === undefined || h.value === item.value));
      },
      // iterate
      each: (fn, ctx) => { snap().forEach((h, i) => fn.call(ctx, h, i)); },
      map: (fn, ctx) => snap().map((h, i) => fn.call(ctx, h, i)),
      reduce: (fn, initial, ctx) => snap().reduce((acc, h, i) => fn.call(ctx, acc, h, i), initial),
      // transform
      toObject: (excludeDisabled, caseSensitive, multiValue, sanitizeKeys) => {
        const out = {};
        for (const h of items) {
          if (excludeDisabled && h.disabled) continue;
          if (sanitizeKeys && !h.key) continue;
          const k = caseSensitive ? h.key : _hkey(h.key);
          if (multiValue) (out[k] = out[k] || []).push(h.value);
          else out[k] = h.value;
        }
        return out;
      },
      toString: () => items.filter((h) => !h.disabled).map((h) => `${h.key}: ${h.value}`).join('\n'),
      toJSON: snap,
      // write
      add: (a, b) => {
        needWriter();
        const h = _toHeader(a, b);
        if (h) put(h);
      },
      upsert: (a, b) => {
        needWriter();
        const h = _toHeader(a, b);
        return h ? put(h) : null;
      },
      remove: (target, ctx) => {
        needWriter();
        if (typeof target === 'function') {
          const keys = snap().filter((h, i) => target.call(ctx, h, i)).map((h) => h.key);
          new Set(keys.map(_hkey)).forEach((k) => dropKey(k));
          return;
        }
        const key = (target && typeof target === 'object') ? target.key : target;
        if (key !== undefined) dropKey(String(key));
      },
      clear: () => {
        needWriter();
        new Set(items.map((h) => _hkey(h.key))).forEach((k) => dropKey(k));
      },
      populate: (src) => {
        needWriter();
        for (const h of _headerItems(src)) if (idx(h.key) < 0) put(h);
      },
      repopulate: (src) => {
        needWriter();
        list.clear();
        list.populate(src);
      },
      assimilate: (src, prune) => {
        needWriter();
        const incoming = _headerItems(src);
        for (const h of incoming) put(h);
        if (prune) {
          const keep = new Set(incoming.map((h) => _hkey(h.key)));
          new Set(items.map((h) => _hkey(h.key))).forEach((k) => { if (!keep.has(k)) dropKey(k); });
        }
      },
    };
    return list;
  }

  // ── req ─────────────────────────────────────────────────────────────────────
  let _reqHeaderList = null;
  const _reqHeaders = () => _reqHeaderList || (_reqHeaderList = _makeHeaderList(
    JSON.parse(__ops.op_req_get_header_list()),
    {
      set: (k, v) => __ops.op_req_set_header(k, v),
      del: (k) => __ops.op_req_delete_header(k),
    },
  ));

  globalThis.req = {
    getUrl:              ()           => __ops.op_req_get_url(),
    setUrl:              (url)        => __ops.op_req_set_url(url),
    getHost:             ()           => __ops.op_req_get_host(),
    getPath:             ()           => __ops.op_req_get_path(),
    getQueryString:      ()           => __ops.op_req_get_query_string(),
    getPathParams:       ()           => JSON.parse(__ops.op_req_get_path_params())
                                          .map((p) => Object.assign({}, p, { type: 'path' })),
    getMethod:           ()           => __ops.op_req_get_method(),
    setMethod:           (method)     => __ops.op_req_set_method(method),
    getName:             ()           => __ops.op_req_get_name(),
    getTags:             ()           => JSON.parse(__ops.op_req_get_tags()),
    getAuthMode:         ()           => __ops.op_req_get_auth_mode(),
    getHeader:           (name)       => {
      const h = _reqHeaders().find((x) => !x.disabled && _hkey(x.key) === _hkey(name));
      return h ? h.value : undefined;
    },
    getHeaders:          ()           => _reqHeaders().toObject(true),
    setHeader:           (name, val)  => { _reqHeaders().upsert({ key: name, value: val }); },
    setHeaders:          (headers)    => {
      const list = _reqHeaders();
      for (const k of Object.keys(headers || {})) list.upsert({ key: k, value: headers[k] });
    },
    deleteHeader:        (name)       => { _reqHeaders().remove(String(name)); },
    deleteHeaders:       (names)      => {
      const list = _reqHeaders();
      for (const n of (names || [])) list.remove(String(n));
    },
    getBody:             (opts)       => {
      const raw = __ops.op_req_get_body();
      if (opts && opts.raw) return raw;
      if (raw === '') return undefined;
      try { return JSON.parse(raw); } catch { return raw; }
    },
    setBody:             (body)       => __ops.op_req_set_body(JSON.stringify(body)),
    getTimeout:          ()           => __ops.op_req_get_timeout(),
    setTimeout:          (ms)         => __ops.op_req_set_timeout(ms),
    setMaxRedirects:     (n)          => __ops.op_req_set_max_redirects(n),
    getExecutionMode:    ()           => __ops.op_req_get_execution_mode(),
    getExecutionPlatform:()           => __ops.op_req_get_execution_platform(),
    onFail:              (_cb)        => { /* no-op in safe mode */ },
  };
  Object.defineProperty(globalThis.req, 'headerList', { get: _reqHeaders, enumerable: false });

  // ── res ─────────────────────────────────────────────────────────────────────
  // Properties are non-enumerable lazy getters, so a script that logs or
  // serialises `res` in the before-request phase does not hit the "res is not
  // available" error; only reading a property does.
  let _resHeaderList = null;
  const _resHeaders = () => _resHeaderList || (_resHeaderList = _makeHeaderList(
    JSON.parse(__ops.op_res_get_header_list()),
    null,
  ));
  const _resBody = (raw) => { try { return JSON.parse(raw); } catch { return raw; } };

  // A body set by res.setBody replaces the stored one for the rest of this script.
  let _resBodyOverride = null;
  const _rawBody = () => (_resBodyOverride !== null ? _resBodyOverride : __ops.op_res_get_body());

  globalThis.res = {
    getStatus:        ()      => __ops.op_res_get_status(),
    getStatusText:    ()      => __ops.op_res_get_status_text(),
    getHeader:        (name)  => _resHeaders().get(name),
    getHeaders:       ()      => _resHeaders().toObject(),
    getBody:          (opts)  => {
      const raw = _rawBody();
      return (opts && opts.raw) ? raw : _resBody(raw);
    },
    setBody:          (body)  => {
      const raw = typeof body === 'string' ? body : JSON.stringify(body);
      __ops.op_res_set_body(raw);
      _resBodyOverride = raw;
    },
    getUrl:           ()      => __ops.op_res_get_url(),
    getSize:          ()      => JSON.parse(__ops.op_res_get_size()),
    getResponseTime:  ()      => __ops.op_res_get_response_time(),
  };
  Object.defineProperties(globalThis.res, {
    status:       { get: () => __ops.op_res_get_status(), enumerable: false },
    statusText:   { get: () => __ops.op_res_get_status_text(), enumerable: false },
    headers:      { get: () => _resHeaders().toObject(), enumerable: false },
    body:         { get: () => _resBody(_rawBody()), enumerable: false },
    url:          { get: () => __ops.op_res_get_url(), enumerable: false },
    responseTime: { get: () => __ops.op_res_get_response_time(), enumerable: false },
    headerList:   { get: _resHeaders, enumerable: false },
  });

  // ── navigator polyfill ───────────────────────────────────────────────────────
  // jsrsasign's legacy PRNG-seeding code (from jsbn) reads navigator.appName /
  // navigator.appVersion unconditionally at module load, with no typeof guard.
  if (typeof globalThis.navigator === 'undefined') {
    globalThis.navigator = { appName: 'Netscape', appVersion: '5.0', userAgent: 'RocketAPI' };
  }

  // ── atob/btoa polyfill ──────────────────────────────────────────────────────
  // Bare deno_core has no deno_web extension, so these globals don't exist by
  // default. Guarded so a future deno_web addition would take precedence.
  if (typeof globalThis.btoa === 'undefined') {
    const _B64_CHARS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
    globalThis.btoa = function(input) {
      const str = String(input);
      let output = '';
      for (let i = 0; i < str.length; i += 3) {
        const a = str.charCodeAt(i);
        const b = i + 1 < str.length ? str.charCodeAt(i + 1) : NaN;
        const c = i + 2 < str.length ? str.charCodeAt(i + 2) : NaN;
        if (a > 255 || (!Number.isNaN(b) && b > 255) || (!Number.isNaN(c) && c > 255)) {
          throw new Error('InvalidCharacterError: btoa input contains characters outside of the Latin1 range');
        }
        const triplet = (a << 16) | ((Number.isNaN(b) ? 0 : b) << 8) | (Number.isNaN(c) ? 0 : c);
        output += _B64_CHARS[(triplet >> 18) & 0x3f];
        output += _B64_CHARS[(triplet >> 12) & 0x3f];
        output += Number.isNaN(b) ? '=' : _B64_CHARS[(triplet >> 6) & 0x3f];
        output += Number.isNaN(c) ? '=' : _B64_CHARS[triplet & 0x3f];
      }
      return output;
    };
  }
  if (typeof globalThis.atob === 'undefined') {
    const _B64_CHARS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
    globalThis.atob = function(input) {
      const str = String(input).replace(/=+$/, '');
      if (str.length % 4 === 1) {
        throw new Error('InvalidCharacterError: atob input is not correctly encoded');
      }
      let output = '';
      let buffer = 0;
      let bits = 0;
      for (let i = 0; i < str.length; i++) {
        const idx = _B64_CHARS.indexOf(str[i]);
        if (idx === -1) throw new Error('InvalidCharacterError: atob input is not correctly encoded');
        buffer = (buffer << 6) | idx;
        bits += 6;
        if (bits >= 8) {
          bits -= 8;
          output += String.fromCharCode((buffer >> bits) & 0xff);
        }
      }
      return output;
    };
  }

  // ── require() module loader ───────────────────────────────────────────────────
  // The new Function(...) body's only lexical parent is the global scope, so a
  // vendored module cannot see `__ops` — and does not need to. None of them
  // reference the Deno global; they use globalThis.crypto, navigator, btoa and
  // atob, all of which survive this file untouched.
  //
  // Bare names load vendored modules. `./x`, `../x` and absolute paths load local
  // `.js` files through op_require_local, which enforces the allowed roots.
  const isLocalSpecifier = (name) =>
    name === '.' ||
    name === '..' ||
    name.startsWith('./') ||
    name.startsWith('../') ||
    name.startsWith('/') ||
    name.startsWith('.\\') ||
    name.startsWith('..\\');

  const loadBundled = function(name) {
    const src = __ops.op_require_module(name);
    if (!src) throw new Error(`Module not found: ${name}`);
    const mod = { exports: {} };
    const fn = new Function("module", "exports", "require", src);
    fn(mod, mod.exports, globalThis.require);
    return mod.exports;
  };

  // One entry per canonical file path for the lifetime of this script run. The
  // entry is stored before the module body runs, so circular requires see the
  // partial exports. A module that throws is evicted so a later require retries.
  const localCache = new Map();

  const makeRequire = function(fromDir) {
    return function require(name) {
      if (typeof name !== 'string') {
        throw new TypeError('require() expects a string module name');
      }
      if (isLocalSpecifier(name)) return loadLocal(fromDir, name);
      return loadBundled(name);
    };
  };

  // Records which modules already named themselves on an error object, so the
  // same error passing the same require frame again is not prefixed twice.
  const prefixedBy = new WeakMap();

  const nameModuleInError = function(e, name, path) {
    const base = path.split(/[\\/]/).pop();
    const prefix = `Error in module '${name}' (${base}): `;
    const isObj = e !== null && (typeof e === 'object' || typeof e === 'function');
    if (isObj && prefixedBy.get(e)?.has(path)) return e;
    let out = e;
    if (e instanceof Error) {
      try {
        e.message = prefix + e.message;
      } catch (_) {
        out = null;
      }
      if (out !== null && !String(e.message).startsWith(prefix)) out = null;
      if (out === null) out = new Error(prefix + e.message, { cause: e });
    } else {
      out = new Error(prefix + String(e), { cause: e });
    }
    prefixedBy.set(out, new Set([...(prefixedBy.get(e) || []), path]));
    return out;
  };

  const loadLocal = function(fromDir, name) {
    const info = JSON.parse(__ops.op_require_local(fromDir, name));
    const cached = localCache.get(info.path);
    if (cached) return cached.exports;
    const mod = { exports: {} };
    localCache.set(info.path, mod);
    try {
      const fn = new Function(
        "module", "exports", "require", "__filename", "__dirname", info.source
      );
      fn(mod, mod.exports, makeRequire(info.dir), info.path, info.dir);
    } catch (e) {
      localCache.delete(info.path);
      throw nameModuleInError(e, name, info.path);
    }
    return mod.exports;
  };

  globalThis.require = makeRequire('');

  // ── test() + expect() ────────────────────────────────────────────────────────
  // Delegate to bundled Chai for full API parity.
  const _chai = require('chai');
  globalThis.expect = function(actual) {
    const assertion = _chai.expect(actual);
    // jest-style alias — not in Chai natively.
    assertion.toBe = (expected) => _chai.expect(actual).to.equal(expected);
    return assertion;
  };

  globalThis.test = function(name, fn) {
    __ops.op_test_run(name);
    try {
      fn();
      __ops.op_test_pass(name);
    } catch (e) {
      __ops.op_test_fail(name, String(e));
    }
  };

  // rok.test / rok.expect aliases so both calling styles work.
  globalThis.rok.test   = globalThis.test;
  globalThis.rok.expect = globalThis.expect;

  // ── fs / process (Developer Mode only) ────────────────────────────────────
  // These ops only exist in the isolate when the collection's sandbox mode is
  // Developer (see rocket_scripting_dev_ext in engine.rs) — feature-detected
  // here rather than assumed, so Safe Mode leaves both globals entirely
  // undefined instead of defined-but-throwing.
  if (typeof __ops.op_fs_read_file === 'function') {
    globalThis.fs = {
      readFile:  (path, opts)          => __ops.op_fs_read_file(path, (opts && opts.encoding) || 'utf8'),
      writeFile: (path, content, opts) => __ops.op_fs_write_file(path, content, (opts && opts.encoding) || 'utf8'),
      readDir:   (path)                => JSON.parse(__ops.op_fs_read_dir(path)),
      exists:    (path)                => __ops.op_fs_exists(path),
      mkdir:     (path, opts)          => __ops.op_fs_mkdir(path, !!(opts && opts.recursive)),
      remove:    (path, opts)          => __ops.op_fs_remove(path, !!(opts && opts.recursive)),
    };
  }
  if (typeof __ops.op_process_exec === 'function') {
    globalThis.process = {
      exec: (command, args, opts) => JSON.parse(__ops.op_process_exec(
        command,
        JSON.stringify(args || []),
        (opts && opts.cwd) || '',
        JSON.stringify((opts && opts.env) || {}),
        (opts && opts.timeoutMs) || 5000,
      )),
    };
  }

  // Every global is wired up now. Remove the raw Deno global so the user script,
  // which runs in a later and separate execute_script() call, cannot reach any
  // deno_core built-in op such as op_print or op_panic directly.
  // KEEP THIS LAST. Anything added below it would still resolve the Deno global
  // and would silently reintroduce the call-time lookup bug this file exists to fix.
  delete globalThis.Deno;

  // deno_core's own setup (00_primordials.js, 00_infra.js, 01_core.js) also
  // parks the same `core` object -- and therefore the same ops table -- on
  // globalThis.__bootstrap.core, via ObjectAssign(globalThis.Deno.core, {...}),
  // which returns its target. deno_core never deletes that handle itself, so
  // without this line globalThis.__bootstrap.core.ops.op_print/op_panic would
  // still reach the same ops table Deno.core.ops did.
  delete globalThis.__bootstrap;
})();
