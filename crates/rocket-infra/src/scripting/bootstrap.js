"use strict";

// ── console ──────────────────────────────────────────────────────────────────
const console = {
  log:   (...args) => Deno.core.ops.op_console_log(args.map(String).join(" ")),
  warn:  (...args) => Deno.core.ops.op_console_warn(args.map(String).join(" ")),
  error: (...args) => Deno.core.ops.op_console_error(args.map(String).join(" ")),
};
globalThis.console = console;

// ── rok ───────────────────────────────────────────────────────────────────────
globalThis.rok = {
  getVar:            (key)        => Deno.core.ops.op_rok_get_var(key),
  setVar:            (key, value) => Deno.core.ops.op_rok_set_var(key, JSON.stringify(value)),
  getEnvVar:         (key)        => Deno.core.ops.op_rok_get_env_var(key),
  setEnvVar:         (key, value, opts) => Deno.core.ops.op_rok_set_env_var(key, JSON.stringify(value), !!(opts && opts.persist)),
  hasEnvVar:         (key)        => Deno.core.ops.op_rok_has_env_var(key),
  deleteEnvVar:      (key)        => Deno.core.ops.op_rok_delete_env_var(key),
  getEnvName:        ()           => Deno.core.ops.op_rok_get_env_name(),
  getCollectionVar:  (key)        => Deno.core.ops.op_rok_get_collection_var(key),
  setCollectionVar:  (key, value) => Deno.core.ops.op_rok_set_collection_var(key, JSON.stringify(value)),
  getGlobalEnvVar:   (key)        => Deno.core.ops.op_rok_get_global_env_var(key),
  setGlobalEnvVar:   (key, value) => Deno.core.ops.op_rok_set_global_env_var(key, JSON.stringify(value)),
  interpolate:       (template)   => Deno.core.ops.op_rok_interpolate(template),
  runner: {
    setNextRequest: (name)  => Deno.core.ops.op_rok_set_next_request(name),
    skipRequest:    ()      => Deno.core.ops.op_rok_skip_request(),
  },
};

// ── req ───────────────────────────────────────────────────────────────────────
globalThis.req = {
  getUrl:              ()           => Deno.core.ops.op_req_get_url(),
  setUrl:              (url)        => Deno.core.ops.op_req_set_url(url),
  getHost:             ()           => Deno.core.ops.op_req_get_host(),
  getPath:             ()           => Deno.core.ops.op_req_get_path(),
  getQueryString:      ()           => Deno.core.ops.op_req_get_query_string(),
  getPathParams:       ()           => JSON.parse(Deno.core.ops.op_req_get_path_params()),
  getMethod:           ()           => Deno.core.ops.op_req_get_method(),
  setMethod:           (method)     => Deno.core.ops.op_req_set_method(method),
  getName:             ()           => Deno.core.ops.op_req_get_name(),
  getTags:             ()           => JSON.parse(Deno.core.ops.op_req_get_tags()),
  getAuthMode:         ()           => Deno.core.ops.op_req_get_auth_mode(),
  getHeader:           (name)       => Deno.core.ops.op_req_get_header(name),
  getHeaders:          ()           => JSON.parse(Deno.core.ops.op_req_get_headers()),
  setHeader:           (name, val)  => Deno.core.ops.op_req_set_header(name, val),
  setHeaders:          (headers)    => Deno.core.ops.op_req_set_headers(JSON.stringify(headers)),
  deleteHeader:        (name)       => Deno.core.ops.op_req_delete_header(name),
  deleteHeaders:       (names)      => Deno.core.ops.op_req_delete_headers(JSON.stringify(names)),
  getBody:             (opts)       => {
    const raw = Deno.core.ops.op_req_get_body(!!(opts && opts.raw));
    return (opts && opts.raw) ? raw : JSON.parse(raw);
  },
  setBody:             (body)       => Deno.core.ops.op_req_set_body(JSON.stringify(body)),
  getTimeout:          ()           => Deno.core.ops.op_req_get_timeout(),
  setTimeout:          (ms)         => Deno.core.ops.op_req_set_timeout(ms),
  setMaxRedirects:     (n)          => Deno.core.ops.op_req_set_max_redirects(n),
  getExecutionMode:    ()           => Deno.core.ops.op_req_get_execution_mode(),
  getExecutionPlatform:()           => Deno.core.ops.op_req_get_execution_platform(),
  onFail:              (_cb)        => { /* no-op in safe mode */ },
};

// ── res ───────────────────────────────────────────────────────────────────────
globalThis.res = {
  getStatus:        ()      => Deno.core.ops.op_res_get_status(),
  getStatusText:    ()      => Deno.core.ops.op_res_get_status_text(),
  getHeader:        (name)  => Deno.core.ops.op_res_get_header(name),
  getHeaders:       ()      => JSON.parse(Deno.core.ops.op_res_get_headers()),
  getBody:          (opts)  => {
    const raw = Deno.core.ops.op_res_get_body(!!(opts && opts.raw));
    return (opts && opts.raw) ? raw : (() => { try { return JSON.parse(raw); } catch { return raw; } })();
  },
  getResponseTime:  ()      => Deno.core.ops.op_res_get_response_time(),
};

// ── test() + expect() (chai subset) ──────────────────────────────────────────
function expect(actual) {
  return {
    to: {
      equal: (expected) => {
        if (actual !== expected) throw new Error(`Expected ${JSON.stringify(actual)} to equal ${JSON.stringify(expected)}`);
      },
      be: {
        true:  () => { if (actual !== true)  throw new Error(`Expected true`); },
        false: () => { if (actual !== false) throw new Error(`Expected false`); },
        null:  () => { if (actual !== null)  throw new Error(`Expected null`); },
      },
      include: (val) => {
        if (typeof actual === 'string' && !actual.includes(val)) throw new Error(`Expected "${actual}" to include "${val}"`);
        if (Array.isArray(actual) && !actual.includes(val)) throw new Error(`Expected array to include ${JSON.stringify(val)}`);
      },
      have: {
        property: (key) => {
          if (typeof actual !== 'object' || actual === null || !(key in actual))
            throw new Error(`Expected object to have property "${key}"`);
        },
        status: (code) => {
          if (actual.status !== code) throw new Error(`Expected status ${code}, got ${actual.status}`);
        },
      },
      not: {
        equal: (expected) => {
          if (actual === expected) throw new Error(`Expected ${JSON.stringify(actual)} to not equal ${JSON.stringify(expected)}`);
        },
      },
    },
  };
}
globalThis.expect = expect;

globalThis.test = function(name, fn) {
  Deno.core.ops.op_test_run(name);
  try {
    fn();
    Deno.core.ops.op_test_pass(name);
  } catch (e) {
    Deno.core.ops.op_test_fail(name, String(e));
  }
};

// ── require() module loader ───────────────────────────────────────────────────
globalThis.require = function(name) {
  const src = Deno.core.ops.op_require_module(name);
  if (!src) throw new Error(`Module not found: ${name}`);
  const mod = { exports: {} };
  const fn = new Function("module", "exports", "require", src);
  fn(mod, mod.exports, globalThis.require);
  return mod.exports;
};
