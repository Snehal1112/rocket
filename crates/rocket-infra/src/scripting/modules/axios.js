// axios is not wired to a Rust HTTP bridge in RocketAPI's script sandbox —
// there is no outbound-request transport available from inside a script.
// This stub throws immediately and clearly at call time instead of silently
// loading a browser/Node bundle whose transport adapters (XMLHttpRequest,
// fetch, Node http) don't exist in this bare deno_core sandbox.
var AXIOS_UNSUPPORTED_MESSAGE =
  "axios is not supported in RocketAPI scripts — use rok/req/res instead.";

function axiosUnsupported() {
  throw new Error(AXIOS_UNSUPPORTED_MESSAGE);
}

var axios = axiosUnsupported;
var methods = ["get", "post", "put", "patch", "delete", "head", "options", "request"];
for (var i = 0; i < methods.length; i++) {
  axios[methods[i]] = axiosUnsupported;
}
axios.create = function () {
  return axios;
};

module.exports = axios;
