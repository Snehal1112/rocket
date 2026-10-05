// Typings for the `response` object a Flow wire script can read.
export const WIRE_SCRIPT_TYPES = `
declare const response: {
  /** HTTP status code of the source node. */
  status: number;
  statusText: string;
  headers: Record<string, string>;
  /** Parsed JSON body, or the raw text when it is not JSON. */
  body: any;
  duration_ms: number;
};

/** Modules bundled with the script sandbox. Keep in sync with op_require_module in rocket-infra. */
declare function require(name: 'lodash'): any;
declare function require(name: 'uuid'): any;
declare function require(name: 'moment'): any;
declare function require(name: 'crypto-js'): any;
declare function require(name: 'nanoid'): any;
declare function require(name: 'chai'): any;
declare function require(name: 'jsonwebtoken'): any;
declare function require(name: 'jsrsasign'): any;
declare function require(name: 'tv4'): any;
declare function require(name: 'atob' | 'btoa'): any;
`;
export const WIRE_SCRIPT_TYPES_PATH = 'ts:flow-wire-response.d.ts';
