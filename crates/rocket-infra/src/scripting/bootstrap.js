"use strict";

// ── console ──────────────────────────────────────────────────────────────────
function _fmt(v) {
  if (v === null || v === undefined) return String(v);
  if (typeof v === 'object' || Array.isArray(v)) {
    try { return JSON.stringify(v); } catch { return String(v); }
  }
  return String(v);
}
const console = {
  log:   (...args) => Deno.core.ops.op_console_log(args.map(_fmt).join(" ")),
  warn:  (...args) => Deno.core.ops.op_console_warn(args.map(_fmt).join(" ")),
  error: (...args) => Deno.core.ops.op_console_error(args.map(_fmt).join(" ")),
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

// ── require() module loader ───────────────────────────────────────────────────
globalThis.require = function(name) {
  const src = Deno.core.ops.op_require_module(name);
  if (!src) throw new Error(`Module not found: ${name}`);
  const mod = { exports: {} };
  const fn = new Function("module", "exports", "require", src);
  fn(mod, mod.exports, globalThis.require);
  return mod.exports;
};

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
  Deno.core.ops.op_test_run(name);
  try {
    fn();
    Deno.core.ops.op_test_pass(name);
  } catch (e) {
    Deno.core.ops.op_test_fail(name, String(e));
  }
};

// rok.test / rok.expect aliases so both calling styles work.
rok.test   = globalThis.test;
rok.expect = globalThis.expect;
