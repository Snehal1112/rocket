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

// ── navigator polyfill ───────────────────────────────────────────────────────────
// jsrsasign's legacy PRNG-seeding code (from jsbn) reads navigator.appName /
// navigator.appVersion unconditionally at module load, with no typeof guard.
if (typeof globalThis.navigator === 'undefined') {
  globalThis.navigator = { appName: 'Netscape', appVersion: '5.0', userAgent: 'RocketAPI' };
}

// ── atob/btoa polyfill ──────────────────────────────────────────────────────────
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
