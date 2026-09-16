(function(root, factory) {
  if (typeof module === 'object' && module.exports) {
    module.exports = factory(require);
  } else {
    root.jsonwebtoken = factory(root.require);
  }
}(typeof globalThis !== 'undefined' ? globalThis : this, function(require) {
  var jsrsasign = require('jsrsasign');

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
    } catch (e) { return null; }
  }

  function decodeHeader(token) {
    var parts = (token || '').split('.');
    if (parts.length !== 3) return null;
    try {
      return JSON.parse(base64urlDecode(parts[0]));
    } catch (e) { return null; }
  }

  // Real HMAC/RSA signing via jsrsasign's KJUR.jws.JWS — no more fake pseudo-signature.
  function sign(payload, secret, options) {
    var opts = options || {};
    var alg = opts.algorithm || 'HS256';
    var now = Math.floor(Date.now() / 1000);
    var claims = Object.assign({ iat: now }, payload);
    if (opts.expiresIn) {
      claims.exp = now + (typeof opts.expiresIn === 'number' ? opts.expiresIn : parseInt(opts.expiresIn, 10));
    }
    var sHeader = JSON.stringify({ alg: alg, typ: 'JWT' });
    var sPayload = JSON.stringify(claims);
    return jsrsasign.KJUR.jws.JWS.sign(alg, sHeader, sPayload, secret);
  }

  // Real signature verification — unlike the previous stub, this rejects
  // tampered tokens and tokens signed with a different secret.
  function verify(token, secret) {
    var header = decodeHeader(token);
    if (!header || !header.alg) return false;
    try {
      return jsrsasign.KJUR.jws.JWS.verify(token, secret, [header.alg]);
    } catch (e) {
      return false;
    }
  }

  return { sign: sign, verify: verify, decode: decode };
}));
