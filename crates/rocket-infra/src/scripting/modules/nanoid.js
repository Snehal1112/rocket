(function(root, factory) {
  if (typeof module === 'object' && module.exports) {
    module.exports = factory();
  } else {
    root.nanoid = factory();
  }
}(typeof globalThis !== 'undefined' ? globalThis : this, function() {
  var urlAlphabet = 'useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict';
  function nanoid(size) {
    size = size || 21;
    var id = '';
    var bytes = new Uint8Array(size);
    if (typeof crypto !== 'undefined' && crypto.getRandomValues) {
      crypto.getRandomValues(bytes);
    } else {
      for (var i = 0; i < size; i++) bytes[i] = Math.random() * 256 | 0;
    }
    for (var j = 0; j < size; j++) {
      id += urlAlphabet[bytes[j] & 63];
    }
    return id;
  }
  nanoid.nanoid = nanoid;
  nanoid.customAlphabet = function(alphabet, defaultSize) {
    return function(size) {
      size = size || defaultSize || 21;
      var id = '';
      for (var i = 0; i < size; i++) {
        id += alphabet[Math.random() * alphabet.length | 0];
      }
      return id;
    };
  };
  return nanoid;
}));
