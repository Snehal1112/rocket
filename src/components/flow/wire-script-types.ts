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
`;
export const WIRE_SCRIPT_TYPES_PATH = 'ts:flow-wire-response.d.ts';
