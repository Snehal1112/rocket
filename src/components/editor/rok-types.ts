import chaiTypeDefsRaw from './chai-types.d.ts.txt?raw';

export type ScriptPhase = 'pre-request' | 'post-response' | 'tests';

export interface ScriptSnippetItem {
  label: string;
  code: string;
  kind: 'template' | 'expression';
}

export interface ScriptSnippetSubGroup {
  id: string;
  label: string;
  items: ScriptSnippetItem[];
}

export interface ScriptSnippetGroup {
  id: string;
  label: string;
  items?: ScriptSnippetItem[];
  subGroups?: ScriptSnippetSubGroup[];
}

export const ROK_SNIPPETS: ScriptSnippetGroup[] = [
  {
    id: 'common-tests',
    label: 'Common Tests',
    items: [
      {
        label: 'Status is 200',
        kind: 'template',
        code: `test("Status is 200", () => {\n  expect(res.getStatus()).to.equal(200);\n});`,
      },
      {
        label: 'Status is 2xx',
        kind: 'template',
        code: `test("Status is 2xx", () => {\n  expect(res.getStatus()).to.be.within(200, 299);\n});`,
      },
      {
        label: 'Response time < 200ms',
        kind: 'template',
        code: `test("Response time < 200ms", () => {\n  expect(res.getResponseTime()).to.be.below(200);\n});`,
      },
      {
        label: 'Body has property',
        kind: 'template',
        code: `test("Body has property", () => {\n  const body = res.getBody();\n  expect(body).to.have.property("key");\n});`,
      },
      {
        label: 'Body equals value',
        kind: 'template',
        code: `test("Body equals value", () => {\n  const body = res.getBody();\n  expect(body.key).to.equal("value");\n});`,
      },
      {
        label: 'Body deep equals',
        kind: 'template',
        code: `test("Body deep equals", () => {\n  const body = res.getBody();\n  expect(body).to.deep.equal({ key: "value" });\n});`,
      },
      {
        label: 'Body is array',
        kind: 'template',
        code: `test("Body is array", () => {\n  expect(res.getBody()).to.be.an("array");\n});`,
      },
      {
        label: 'Body is not empty',
        kind: 'template',
        code: `test("Body is not empty", () => {\n  expect(res.getBody()).to.not.be.empty;\n});`,
      },
      {
        label: 'Header exists',
        kind: 'template',
        code: `test("Header exists", () => {\n  expect(res.getHeader("content-type")).to.exist;\n});`,
      },
      {
        label: 'Header equals value',
        kind: 'template',
        code: `test("Header equals value", () => {\n  expect(res.getHeader("content-type")).to.include("application/json");\n});`,
      },
      {
        label: 'Status is 404',
        kind: 'template',
        code: `test("Status is 404", () => {\n  expect(res.getStatus()).to.equal(404);\n});`,
      },
      {
        label: 'Body matches regex',
        kind: 'template',
        code: `test("Body matches regex", () => {\n  const body = res.getBody({ raw: true });\n  expect(body).to.match(/pattern/);\n});`,
      },
    ],
  },
  {
    id: 'api-reference',
    label: 'API Reference',
    subGroups: [
      {
        id: 'res',
        label: 'res.*',
        items: [
          { label: 'res.getStatus()', kind: 'expression', code: 'res.getStatus()' },
          { label: 'res.getStatusText()', kind: 'expression', code: 'res.getStatusText()' },
          { label: 'res.getHeader("name")', kind: 'expression', code: 'res.getHeader("name")' },
          { label: 'res.getHeaders()', kind: 'expression', code: 'res.getHeaders()' },
          { label: 'res.getBody()', kind: 'expression', code: 'res.getBody()' },
          {
            label: 'res.getBody({ raw: true })',
            kind: 'expression',
            code: 'res.getBody({ raw: true })',
          },
          { label: 'res.getResponseTime()', kind: 'expression', code: 'res.getResponseTime()' },
        ],
      },
      {
        id: 'rok',
        label: 'rok.*',
        items: [
          { label: 'rok.getVar("key")', kind: 'expression', code: 'rok.getVar("key")' },
          {
            label: 'rok.setVar("key", value)',
            kind: 'expression',
            code: 'rok.setVar("key", value)',
          },
          { label: 'rok.getEnvVar("key")', kind: 'expression', code: 'rok.getEnvVar("key")' },
          {
            label: 'rok.setEnvVar("key", value)',
            kind: 'expression',
            code: 'rok.setEnvVar("key", value)',
          },
          {
            label: 'rok.getSecretVar("alias.name")',
            kind: 'expression',
            code: 'rok.getSecretVar("alias.name")',
          },
          {
            label: 'rok.getCollectionVar("key")',
            kind: 'expression',
            code: 'rok.getCollectionVar("key")',
          },
          {
            label: 'rok.getFolderVar("key")',
            kind: 'expression',
            code: 'rok.getFolderVar("key")',
          },
          { label: 'rok.getEnvName()', kind: 'expression', code: 'rok.getEnvName()' },
          {
            label: 'rok.interpolate("{{template}}")',
            kind: 'expression',
            code: 'rok.interpolate("{{template}}")',
          },
          {
            label: 'rok.runner.setNextRequest("name")',
            kind: 'expression',
            code: 'rok.runner.setNextRequest("name")',
          },
        ],
      },
      {
        id: 'expect',
        label: 'expect',
        items: [
          { label: '.to.equal(value)', kind: 'expression', code: '.to.equal(value)' },
          { label: '.to.deep.equal(value)', kind: 'expression', code: '.to.deep.equal(value)' },
          { label: '.to.eql(value)', kind: 'expression', code: '.to.eql(value)' },
          { label: '.to.exist', kind: 'expression', code: '.to.exist' },
          { label: '.to.be.ok', kind: 'expression', code: '.to.be.ok' },
          { label: '.to.be.true', kind: 'expression', code: '.to.be.true' },
          { label: '.to.be.false', kind: 'expression', code: '.to.be.false' },
          { label: '.to.be.null', kind: 'expression', code: '.to.be.null' },
          { label: '.to.be.undefined', kind: 'expression', code: '.to.be.undefined' },
          { label: '.to.be.a("type")', kind: 'expression', code: '.to.be.a("type")' },
          { label: '.to.be.an("array")', kind: 'expression', code: '.to.be.an("array")' },
          {
            label: '.to.be.instanceof(Constructor)',
            kind: 'expression',
            code: '.to.be.instanceof(Constructor)',
          },
          { label: '.to.be.above(n)', kind: 'expression', code: '.to.be.above(n)' },
          { label: '.to.be.below(n)', kind: 'expression', code: '.to.be.below(n)' },
          { label: '.to.be.within(min, max)', kind: 'expression', code: '.to.be.within(min, max)' },
          {
            label: '.to.be.closeTo(n, delta)',
            kind: 'expression',
            code: '.to.be.closeTo(n, delta)',
          },
          { label: '.to.be.at.least(n)', kind: 'expression', code: '.to.be.at.least(n)' },
          { label: '.to.be.at.most(n)', kind: 'expression', code: '.to.be.at.most(n)' },
          { label: '.to.include("str")', kind: 'expression', code: '.to.include("str")' },
          { label: '.to.have.length(n)', kind: 'expression', code: '.to.have.length(n)' },
          {
            label: '.to.have.property("key")',
            kind: 'expression',
            code: '.to.have.property("key")',
          },
          {
            label: '.to.have.property("key", value)',
            kind: 'expression',
            code: '.to.have.property("key", value)',
          },
          {
            label: '.to.have.own.property("key")',
            kind: 'expression',
            code: '.to.have.own.property("key")',
          },
          { label: '.to.have.keys("a", "b")', kind: 'expression', code: '.to.have.keys("a", "b")' },
          { label: '.to.have.members([...])', kind: 'expression', code: '.to.have.members([])' },
          { label: '.to.match(/regex/)', kind: 'expression', code: '.to.match(/regex/)' },
          { label: '.to.be.empty', kind: 'expression', code: '.to.be.empty' },
          { label: '.to.satisfy(fn)', kind: 'expression', code: '.to.satisfy((val) => val > 0)' },
          { label: '.to.not.equal(value)', kind: 'expression', code: '.to.not.equal(value)' },
          {
            label: '.to.not.have.property("key")',
            kind: 'expression',
            code: '.to.not.have.property("key")',
          },
          { label: '.to.not.include("str")', kind: 'expression', code: '.to.not.include("str")' },
          { label: '.to.not.be.null', kind: 'expression', code: '.to.not.be.null' },
          { label: '.to.not.be.undefined', kind: 'expression', code: '.to.not.be.undefined' },
          { label: '.to.not.be.empty', kind: 'expression', code: '.to.not.be.empty' },
          { label: '.to.throw()', kind: 'expression', code: '.to.throw()' },
        ],
      },
    ],
  },
];

export const POST_RESPONSE_SNIPPETS: ScriptSnippetGroup[] = [
  {
    id: 'common-patterns',
    label: 'Common Patterns',
    items: [
      {
        label: 'Save body field to env var',
        kind: 'template',
        code: `const value = res.getBody().field;\nrok.setEnvVar("key", value);`,
      },
      {
        label: 'Save header to env var',
        kind: 'template',
        code: `const value = res.getHeader("header-name");\nrok.setEnvVar("key", value);`,
      },
      {
        label: 'Log response body',
        kind: 'template',
        code: `console.log(res.getBody());`,
      },
      {
        label: 'Set collection var from body',
        kind: 'template',
        code: `const value = res.getBody().field;\nrok.setCollectionVar("key", value);`,
      },
      {
        label: 'Set var only if 2xx',
        kind: 'template',
        code: `if (res.getStatus() >= 200 && res.getStatus() < 300) {\n  rok.setEnvVar("key", res.getBody().field);\n}`,
      },
    ],
  },
  {
    id: 'api-reference',
    label: 'API Reference',
    subGroups: [
      {
        id: 'res',
        label: 'res.*',
        items: [
          { label: 'res.getStatus()', kind: 'expression', code: 'res.getStatus()' },
          { label: 'res.getStatusText()', kind: 'expression', code: 'res.getStatusText()' },
          { label: 'res.getHeader("name")', kind: 'expression', code: 'res.getHeader("name")' },
          { label: 'res.getHeaders()', kind: 'expression', code: 'res.getHeaders()' },
          { label: 'res.getBody()', kind: 'expression', code: 'res.getBody()' },
          {
            label: 'res.getBody({ raw: true })',
            kind: 'expression',
            code: 'res.getBody({ raw: true })',
          },
          { label: 'res.getResponseTime()', kind: 'expression', code: 'res.getResponseTime()' },
        ],
      },
      {
        id: 'rok',
        label: 'rok.*',
        items: [
          { label: 'rok.getVar("key")', kind: 'expression', code: 'rok.getVar("key")' },
          {
            label: 'rok.setVar("key", value)',
            kind: 'expression',
            code: 'rok.setVar("key", value)',
          },
          { label: 'rok.getEnvVar("key")', kind: 'expression', code: 'rok.getEnvVar("key")' },
          {
            label: 'rok.setEnvVar("key", value)',
            kind: 'expression',
            code: 'rok.setEnvVar("key", value)',
          },
          {
            label: 'rok.getSecretVar("alias.name")',
            kind: 'expression',
            code: 'rok.getSecretVar("alias.name")',
          },
          {
            label: 'rok.setCollectionVar("key", value)',
            kind: 'expression',
            code: 'rok.setCollectionVar("key", value)',
          },
          {
            label: 'rok.getCollectionVar("key")',
            kind: 'expression',
            code: 'rok.getCollectionVar("key")',
          },
          {
            label: 'rok.getFolderVar("key")',
            kind: 'expression',
            code: 'rok.getFolderVar("key")',
          },
          { label: 'rok.getEnvName()', kind: 'expression', code: 'rok.getEnvName()' },
          {
            label: 'rok.interpolate("{{template}}")',
            kind: 'expression',
            code: 'rok.interpolate("{{template}}")',
          },
          {
            label: 'rok.runner.setNextRequest("name")',
            kind: 'expression',
            code: 'rok.runner.setNextRequest("name")',
          },
        ],
      },
    ],
  },
];

export const PRE_REQUEST_SNIPPETS: ScriptSnippetGroup[] = [
  {
    id: 'common-patterns',
    label: 'Common Patterns',
    items: [
      {
        label: 'Set Authorization header from env var',
        kind: 'template',
        code: `req.setHeader("Authorization", "Bearer " + rok.getEnvVar("token"));`,
      },
      {
        label: 'Set a request header',
        kind: 'template',
        code: `req.setHeader("X-Custom-Header", "value");`,
      },
      {
        label: 'Override the request body',
        kind: 'template',
        code: `req.setBody({ key: "value" });`,
      },
      {
        label: 'Override the request URL',
        kind: 'template',
        code: `req.setUrl("https://example.com/api");`,
      },
      {
        label: 'Log request details',
        kind: 'template',
        code: `console.log(req.getMethod(), req.getUrl());`,
      },
    ],
  },
  {
    id: 'api-reference',
    label: 'API Reference',
    subGroups: [
      {
        id: 'req',
        label: 'req.*',
        items: [
          { label: 'req.getUrl()', kind: 'expression', code: 'req.getUrl()' },
          { label: 'req.setUrl(url)', kind: 'expression', code: 'req.setUrl("url")' },
          { label: 'req.getMethod()', kind: 'expression', code: 'req.getMethod()' },
          { label: 'req.setMethod(method)', kind: 'expression', code: 'req.setMethod("GET")' },
          {
            label: 'req.getHeader("name")',
            kind: 'expression',
            code: 'req.getHeader("name")',
          },
          {
            label: 'req.setHeader("name", value)',
            kind: 'expression',
            code: 'req.setHeader("name", "value")',
          },
          {
            label: 'req.deleteHeader("name")',
            kind: 'expression',
            code: 'req.deleteHeader("name")',
          },
          { label: 'req.getBody()', kind: 'expression', code: 'req.getBody()' },
          { label: 'req.setBody(body)', kind: 'expression', code: 'req.setBody({})' },
          { label: 'req.setTimeout(ms)', kind: 'expression', code: 'req.setTimeout(5000)' },
        ],
      },
      {
        id: 'rok',
        label: 'rok.*',
        items: [
          { label: 'rok.getVar("key")', kind: 'expression', code: 'rok.getVar("key")' },
          {
            label: 'rok.setVar("key", value)',
            kind: 'expression',
            code: 'rok.setVar("key", value)',
          },
          { label: 'rok.getEnvVar("key")', kind: 'expression', code: 'rok.getEnvVar("key")' },
          {
            label: 'rok.getSecretVar("alias.name")',
            kind: 'expression',
            code: 'rok.getSecretVar("alias.name")',
          },
          {
            label: 'rok.getCollectionVar("key")',
            kind: 'expression',
            code: 'rok.getCollectionVar("key")',
          },
          {
            label: 'rok.getFolderVar("key")',
            kind: 'expression',
            code: 'rok.getFolderVar("key")',
          },
          {
            label: 'rok.interpolate("{{template}}")',
            kind: 'expression',
            code: 'rok.interpolate("{{template}}")',
          },
          {
            label: 'rok.runner.setNextRequest("name")',
            kind: 'expression',
            code: 'rok.runner.setNextRequest("name")',
          },
          {
            label: 'rok.runner.skipRequest()',
            kind: 'expression',
            code: 'rok.runner.skipRequest()',
          },
        ],
      },
    ],
  },
];

const ROK_DEFS = `
declare const rok: {
  /** Read a runtime variable set in a previous script. */
  getVar(key: string): unknown;
  /** Set a runtime variable (in-memory, cleared after request). */
  setVar(key: string, value: unknown): void;
  /** Read an environment variable. */
  getEnvVar(key: string): unknown;
  /** Write an environment variable (persisted to the active environment file). */
  setEnvVar(key: string, value: unknown): void;
  /** Returns true if the environment variable exists. */
  hasEnvVar(key: string): boolean;
  /** Delete an environment variable. */
  deleteEnvVar(key: string): void;
  /** Returns the active environment name. */
  getEnvName(): string | undefined;
  /** Display name of the collection this request belongs to. */
  getCollectionName(): string;
  /** True in Safe mode, false in Developer mode. */
  isSafeMode(): boolean;
  /** Absolute path of the collection directory. Developer mode only, throws in Safe mode. */
  cwd(): string;
  /** Read an External Secret fetched from RocketVault, keyed "alias.secretName". Returns "" when unknown. */
  getSecretVar(key: string): string;
  /** Read a collection variable. */
  getCollectionVar(key: string): unknown;
  /** Write a collection variable (persisted to opencollection.yml). */
  setCollectionVar(key: string, value: unknown): void;
  /** Read a folder variable. The innermost folder wins and disabled entries are skipped. Returns "" when unknown. Read-only. */
  getFolderVar(key: string): unknown;
  /** Read a global environment variable. */
  getGlobalEnvVar(key: string): unknown;
  /** Write a global environment variable. */
  setGlobalEnvVar(key: string, value: unknown): void;
  /** Delete a runtime variable. */
  deleteVar(key: string): void;
  /** Delete every runtime variable. */
  deleteAllVars(): void;
  /** Delete every variable of the active environment (persisted). */
  deleteAllEnvVars(): void;
  /** Delete a collection variable (persisted to opencollection.yml). */
  deleteCollectionVar(key: string): void;
  /** Delete every collection variable (persisted). */
  deleteAllCollectionVars(): void;
  /** Delete a global environment variable (persisted). */
  deleteGlobalEnvVar(key: string): void;
  /** Delete every global environment variable (persisted). */
  deleteAllGlobalEnvVars(): void;
  /** Resolve {{var}} tokens using the current variable context. */
  interpolate(template: string): string;
  /** Every variable of the active environment. */
  getAllEnvVars(): Record<string, string>;
  /** Every runtime variable. */
  getAllVars(): Record<string, string>;
  /** Every global environment variable. */
  getAllGlobalEnvVars(): Record<string, string>;
  /** Returns true if the runtime variable exists. */
  hasVar(key: string): boolean;
  /** Returns true if the global environment variable exists. */
  hasGlobalEnvVar(key: string): boolean;
  /** Returns true if the collection variable exists. */
  hasCollectionVar(key: string): boolean;
  /** Read a request variable. */
  getRequestVar(key: string): string;
  /** Read a host environment variable, or undefined when it is not set. */
  getProcessEnv(key: string): string | undefined;
  /** Same as rok.runner.setNextRequest. Pass null to stop the run. */
  setNextRequest(name: string | null): void;
  /**
   * Controls the Collection Runner's sequencing (see
   * docs/superpowers/specs/2026-09-16-collection-runner-design.md). Only
   * has an effect when the script runs as part of a run driven by
   * CollectionRunnerService (crates/rocket-app/src/collection_runner_service.rs)
   * — a no-op outside a run, and outside a single request send.
   */
  runner: {
    /** Jump to the named request next instead of the next item in sequence, or pass null to stop the run. Only meaningful during a Collection Runner run. */
    setNextRequest(name: string | null): void;
    /** Skip this request during a Collection Runner run — no HTTP call is made and no later phase runs for this step. Only meaningful from the before-request phase. */
    skipRequest(): void;
  };
};
/** Collection directory. Developer mode only. */
declare const __dirname: string;
/** Always undefined in top-level scripts. */
declare const __filename: string | undefined;
`;

const HEADER_LIST_DEFS = `
interface RokHeader { key: string; value: string; disabled?: boolean }
interface RokReadonlyHeaderList {
  /** Value of the first header with this key (case-insensitive), or undefined. */
  get(name: string): string | undefined;
  /** The full { key, value } object for this key, or undefined. */
  one(name: string): RokHeader | undefined;
  /** A copy of every header, including disabled ones. */
  all(): RokHeader[];
  /** Number of headers, including disabled ones. */
  count(): number;
  has(name: string, value?: string): boolean;
  has(header: { key: string }): boolean;
  find(fn: (header: RokHeader, index: number) => unknown, context?: unknown): RokHeader | undefined;
  filter(fn: (header: RokHeader, index: number) => unknown, context?: unknown): RokHeader[];
  indexOf(item: string | RokHeader): number;
  each(fn: (header: RokHeader, index: number) => void, context?: unknown): void;
  map<T>(fn: (header: RokHeader, index: number) => T, context?: unknown): T[];
  reduce<T>(fn: (acc: T, header: RokHeader, index: number) => T, initial?: T, context?: unknown): T;
  toObject(excludeDisabled?: boolean, caseSensitive?: boolean, multiValue?: boolean, sanitizeKeys?: boolean): Record<string, string | string[]>;
  /** HTTP wire format, "Key: Value" per line, skipping disabled headers. */
  toString(): string;
  toJSON(): RokHeader[];
}
interface RokHeaderList extends RokReadonlyHeaderList {
  /** Sets a header. Takes (name, value), a "Key: Value" string, or a { key, value } object. Overwrites a header with the same key. */
  add(name: string, value?: string): void;
  add(header: RokHeader): void;
  /** Sets or replaces a header. Returns true when it was new, false when updated. */
  upsert(name: string, value?: string): boolean | null;
  upsert(header: RokHeader): boolean | null;
  /** Removes headers by key, { key } object or predicate. */
  remove(target: string | { key: string } | ((header: RokHeader, index: number) => unknown), context?: unknown): void;
  clear(): void;
  /** Adds headers from an array or multi-line string, skipping keys that already exist. */
  populate(items: RokHeader[] | string): void;
  /** Clears all headers, then adds the given ones. */
  repopulate(items: RokHeader[] | string): void;
  /** Sets every header from the source. With prune, removes headers that are not in the source. */
  assimilate(source: RokReadonlyHeaderList | RokHeader[], prune?: boolean): void;
}
`;

const RES_DEFS =
  HEADER_LIST_DEFS +
  `
declare const res: {
  /** Returns the HTTP status code (e.g. 200). */
  getStatus(): number;
  /** Returns the HTTP status text (e.g. "OK"). */
  getStatusText(): string;
  /** Returns the value of a response header (case-insensitive), or undefined when missing. */
  getHeader(name: string): string | undefined;
  /** Returns all response headers as a key-value record with lowercased keys. */
  getHeaders(): Record<string, string>;
  /** Returns the parsed response body. Pass { raw: true } for the raw string. */
  getBody(opts?: { raw?: boolean }): unknown;
  /** Returns the total response time in milliseconds. */
  getResponseTime(): number;
  /** The HTTP status code. Same as getStatus(). */
  readonly status: number;
  /** The HTTP status text. Same as getStatusText(). */
  readonly statusText: string;
  /** All response headers with lowercased keys. Same as getHeaders(). */
  readonly headers: Record<string, string>;
  /** The response body, parsed as JSON when possible, otherwise the raw string. */
  readonly body: any;
  /** The total response time in milliseconds. Same as getResponseTime(). */
  readonly responseTime: number;
  /** Read-only header list. Its write methods throw "HeaderList is read-only". */
  readonly headerList: RokReadonlyHeaderList;
};
`;

const REQ_DEFS =
  HEADER_LIST_DEFS +
  `
declare const req: {
  getUrl(): string;
  setUrl(url: string): void;
  getHost(): string;
  getPath(): string;
  getQueryString(): string;
  /** Path params extracted from the URL (e.g. :id in /users/:id). */
  getPathParams(): { name: string; value: string; type: 'path' }[];
  getMethod(): string;
  setMethod(method: string): void;
  getName(): string;
  /** Tags configured on this request. */
  getTags(): string[];
  /** "none", "inherit", "basic", "bearer", "apikey", "oauth2", "oauth1", "awsv4", "digest", "wsse" or "ntlm". */
  getAuthMode(): string;
  /** Value of an enabled request header (case-insensitive), or undefined when missing. */
  getHeader(name: string): string | undefined;
  /** Enabled request headers with lowercased keys. */
  getHeaders(): Record<string, string>;
  /** Header list with read, search, iterate, transform and write methods. Writes take effect before the request is sent. */
  readonly headerList: RokHeaderList;
  setHeader(name: string, value: string): void;
  setHeaders(headers: Record<string, string>): void;
  deleteHeader(name: string): void;
  deleteHeaders(names: string[]): void;
  /** The body parsed as JSON when possible, otherwise the raw string; undefined when there is no body. Pass { raw: true } for the raw string. */
  getBody(opts?: { raw?: boolean }): unknown;
  setBody(body: unknown): void;
  getTimeout(): number;
  setTimeout(ms: number): void;
  setMaxRedirects(n: number): void;
  getExecutionMode(): "runner" | "standalone";
  getExecutionPlatform(): "app";
  /** Reserved — always a no-op today, does not register a handler of any kind. */
  onFail(callback: (error: unknown) => void): void;
};
`;

// Strip triple-slash reference directives that Monaco cannot resolve at runtime.
const CHAI_TYPE_DEFS = (chaiTypeDefsRaw as string).replace(/\/\/\/\s*<reference[^>]*>\s*\n/g, '');

const TEST_DEFS = `
${CHAI_TYPE_DEFS}

/** Register a named assertion block. Each block runs independently. */
declare function test(name: string, fn: () => void): void;

/** Full Chai expect — chain assertions with .to.equal(), .to.have.property(), .to.match(), etc. */
declare const expect: Chai.ExpectStatic;
`;

/** Returns the Monaco extra-lib `.d.ts` string for the given script phase. */
export function ROK_TYPE_DEFS_FOR_PHASE(phase: ScriptPhase): string {
  switch (phase) {
    case 'pre-request':
      return ROK_DEFS + REQ_DEFS;
    case 'post-response':
      return ROK_DEFS + RES_DEFS;
    case 'tests':
      return ROK_DEFS + RES_DEFS + TEST_DEFS;
  }
}
