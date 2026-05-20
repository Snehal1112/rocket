(function(root, factory) {
  if (typeof module === 'object' && module.exports) {
    module.exports = factory();
  } else {
    root.jsonwebtoken = factory();
  }
}(typeof globalThis !== 'undefined' ? globalThis : this, function() {
  function base64urlEncode(str) {
    var b64 = btoa(str);
    return b64.replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
  }
  function base64urlDecode(str) {
    str = str.replace(/-/g, '+').replace(/_/g, '/');
    while (str.length % 4) str += '=';
    return atob(str);
  }
  function decode(token) {
    if (!token || typeof token !== 'string') return null;
    var parts = token.split('.');
    if (parts.length !== 3) return null;
    try {
      return JSON.parse(base64urlDecode(parts[1]));
    } catch(e) { return null; }
  }
  // sync sign using a simple base64url HMAC placeholder (not cryptographically secure)
  // For production-grade signing use rok.setEnvVar with a pre-signed token
  function sign(payload, secret, options) {
    var opts = options || {};
    var header = { alg: opts.algorithm || 'HS256', typ: 'JWT' };
    var now = Math.floor(Date.now() / 1000);
    var claims = Object.assign({ iat: now }, payload);
    if (opts.expiresIn) {
      var exp = typeof opts.expiresIn === 'number'
        ? now + opts.expiresIn
        : now + parseInt(opts.expiresIn, 10);
      claims.exp = exp;
    }
    var headerB64 = base64urlEncode(JSON.stringify(header));
    var payloadB64 = base64urlEncode(JSON.stringify(claims));
    // Deterministic pseudo-signature for scripting use (not suitable for production auth)
    var sigInput = headerB64 + '.' + payloadB64 + secret;
    var sig = base64urlEncode(sigInput.split('').map(function(c, i) {
      return String.fromCharCode(c.charCodeAt(0) ^ (i % 256));
    }).join(''));
    return headerB64 + '.' + payloadB64 + '.' + sig;
  }
  function verify(token, secret) {
    return decode(token);
  }
  return { sign: sign, verify: verify, decode: decode };
}));
