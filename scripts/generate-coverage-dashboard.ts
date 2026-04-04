#!/usr/bin/env tsx
/**
 * generate-coverage-dashboard.ts
 *
 * Reads:
 *   - coverage/frontend/coverage-summary.json  (Vitest / V8)
 *   - coverage/rust/coverage-summary.json       (cargo-llvm-cov --json)
 *
 * Writes:
 *   - coverage/index.html  (combined human-readable dashboard)
 */

import { execSync } from 'child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs';
import { join } from 'path';

const ROOT = new URL('..', import.meta.url).pathname;
const COVERAGE_DIR = join(ROOT, 'coverage');

// ─── helpers ────────────────────────────────────────────────────────────────

function pct(covered: number, total: number): number {
  return total === 0 ? 100 : Math.round((covered / total) * 1000) / 10;
}

function colorClass(p: number): string {
  if (p >= 80) return 'good';
  if (p >= 60) return 'warn';
  return 'bad';
}

function bar(p: number): string {
  const cls = colorClass(p);
  return `<div class="bar-wrap"><div class="bar ${cls}" style="width:${p}%"></div><span>${p}%</span></div>`;
}

// ─── frontend coverage ──────────────────────────────────────────────────────

interface V8FileSummary {
  lines: { total: number; covered: number; pct: number };
  statements: { total: number; covered: number; pct: number };
  functions: { total: number; covered: number; pct: number };
  branches: { total: number; covered: number; pct: number };
}

interface V8Summary {
  [file: string]: V8FileSummary;
}

function loadFrontendCoverage(): {
  rows: { file: string; lines: number; stmts: number; funcs: number; branches: number }[];
  totals: V8FileSummary | null;
} {
  const summaryPath = join(COVERAGE_DIR, 'frontend', 'coverage-summary.json');
  if (!existsSync(summaryPath)) {
    return { rows: [], totals: null };
  }

  const raw: V8Summary = JSON.parse(readFileSync(summaryPath, 'utf8'));
  const totals = raw['total'] ?? null;
  const rows = Object.entries(raw)
    .filter(([k]) => k !== 'total')
    .map(([file, s]) => ({
      file: file.replace(ROOT, ''),
      lines: s.lines.pct,
      stmts: s.statements.pct,
      funcs: s.functions.pct,
      branches: s.branches.pct,
    }))
    .sort((a, b) => a.lines - b.lines); // lowest coverage first

  return { rows, totals };
}

// ─── rust coverage ──────────────────────────────────────────────────────────

interface LlvmFileCov {
  filename: string;
  summary: {
    lines: { covered: number; count: number; percent: number };
    functions: { covered: number; count: number; percent: number };
    regions: { covered: number; count: number; percent: number };
    branches: { covered: number; count: number; percent: number };
  };
}

interface LlvmReport {
  data: Array<{ files: LlvmFileCov[]; totals: LlvmFileCov['summary'] }>;
}

function loadRustCoverage(): {
  rows: { file: string; lines: number; funcs: number; regions: number }[];
  totals: LlvmFileCov['summary'] | null;
} {
  const summaryPath = join(COVERAGE_DIR, 'rust', 'coverage-summary.json');
  if (!existsSync(summaryPath)) {
    return { rows: [], totals: null };
  }

  const raw: LlvmReport = JSON.parse(readFileSync(summaryPath, 'utf8'));
  const data = raw.data?.[0];
  if (!data) return { rows: [], totals: null };

  const rows = data.files
    .map((f) => ({
      file: f.filename.replace(ROOT, '').replace(/^\//, ''),
      lines: Math.round(f.summary.lines.percent * 10) / 10,
      funcs: Math.round(f.summary.functions.percent * 10) / 10,
      regions: Math.round(f.summary.regions.percent * 10) / 10,
    }))
    .filter((r) => !r.file.includes('/.cargo/') && !r.file.includes('/target/'))
    .sort((a, b) => a.lines - b.lines);

  return { rows, totals: data.totals };
}

// ─── test counts from cargo test output ─────────────────────────────────────

interface CrateResult {
  crate: string;
  passed: number;
  failed: number;
  ignored: number;
}

function runCargoTests(): CrateResult[] {
  console.log('Running cargo test --workspace (for test counts)...');
  try {
    const out = execSync('cargo test --workspace 2>&1', {
      cwd: ROOT,
      encoding: 'utf8',
      timeout: 300_000,
    });

    const results: CrateResult[] = [];
    // Matches: "     Running unittests src/lib.rs (target/debug/deps/rocket_app-abc123)"
    const binaryRe = /Running unittests .+\(target\/\w+\/deps\/([a-zA-Z_]+)-[0-9a-f]+\)/;
    const resultRe = /test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored/;

    const lines = out.split('\n');
    let currentCrate = '';
    for (const line of lines) {
      const binaryMatch = line.match(binaryRe);
      if (binaryMatch) {
        currentCrate = binaryMatch[1].replace(/_/g, '-');
        continue;
      }
      const resultMatch = line.match(resultRe);
      if (resultMatch && currentCrate) {
        const passed = parseInt(resultMatch[2]);
        const failed = parseInt(resultMatch[3]);
        const ignored = parseInt(resultMatch[4]);
        if (passed + failed + ignored > 0) {
          results.push({ crate: currentCrate, passed, failed, ignored });
        }
        currentCrate = '';
      }
    }
    return results;
  } catch {
    return [];
  }
}

// ─── HTML generation ─────────────────────────────────────────────────────────

const CSS = `
  :root { --bg: #0f1117; --card: #1a1d27; --border: #2e3148; --text: #e2e8f0; --muted: #94a3b8; }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: var(--bg); color: var(--text); font: 14px/1.6 system-ui, sans-serif; padding: 24px; }
  h1 { font-size: 22px; font-weight: 700; margin-bottom: 4px; }
  .subtitle { color: var(--muted); font-size: 13px; margin-bottom: 28px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 16px; margin-bottom: 28px; }
  .card { background: var(--card); border: 1px solid var(--border); border-radius: 10px; padding: 18px; }
  .card .label { font-size: 12px; color: var(--muted); text-transform: uppercase; letter-spacing: .05em; margin-bottom: 6px; }
  .card .value { font-size: 28px; font-weight: 700; }
  .card .value.good { color: #4ade80; }
  .card .value.warn { color: #facc15; }
  .card .value.bad { color: #f87171; }
  .card .value.neutral { color: var(--text); }
  .section { background: var(--card); border: 1px solid var(--border); border-radius: 10px; margin-bottom: 28px; overflow: hidden; }
  .section-header { padding: 14px 18px; border-bottom: 1px solid var(--border); font-weight: 600; font-size: 15px; display: flex; align-items: center; gap: 8px; }
  .badge { font-size: 11px; font-weight: 500; padding: 2px 8px; border-radius: 99px; }
  .badge.ok { background: #166534; color: #bbf7d0; }
  .badge.fail { background: #7f1d1d; color: #fecaca; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  th { padding: 10px 18px; text-align: left; color: var(--muted); font-weight: 500; font-size: 12px; background: rgba(255,255,255,.02); position: sticky; top: 0; }
  td { padding: 8px 18px; border-top: 1px solid var(--border); }
  tr:hover td { background: rgba(255,255,255,.03); }
  .bar-wrap { display: flex; align-items: center; gap: 8px; }
  .bar { height: 6px; border-radius: 3px; min-width: 2px; }
  .bar.good { background: #4ade80; }
  .bar.warn { background: #facc15; }
  .bar.bad { background: #f87171; }
  .bar-wrap span { font-size: 12px; color: var(--muted); white-space: nowrap; }
  .file-name { font-family: monospace; font-size: 12px; color: var(--muted); max-width: 420px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .links { padding: 16px 18px; display: flex; gap: 12px; }
  .link-btn { display: inline-block; padding: 8px 16px; background: #2e3148; border-radius: 6px; color: var(--text); text-decoration: none; font-size: 13px; }
  .link-btn:hover { background: #3d4166; }
  .empty { padding: 32px; text-align: center; color: var(--muted); font-size: 13px; }
  .ts { color: var(--muted); font-size: 12px; }
`;

function generateHTML(
  fe: ReturnType<typeof loadFrontendCoverage>,
  rs: ReturnType<typeof loadRustCoverage>,
  tests: CrateResult[],
  generatedAt: Date,
): string {
  const totalPassed = tests.reduce((s, r) => s + r.passed, 0);
  const totalFailed = tests.reduce((s, r) => s + r.failed, 0);
  const totalIgnored = tests.reduce((s, r) => s + r.ignored, 0);

  const feLinesTotal = fe.totals?.lines ?? null;
  const rsLinesTotal = rs.totals?.lines ?? null;

  const summaryCards = `
    <div class="grid">
      <div class="card">
        <div class="label">Tests Passed</div>
        <div class="value ${totalFailed === 0 ? 'good' : 'bad'}">${totalPassed}</div>
      </div>
      <div class="card">
        <div class="label">Tests Failed</div>
        <div class="value ${totalFailed === 0 ? 'neutral' : 'bad'}">${totalFailed}</div>
      </div>
      <div class="card">
        <div class="label">Ignored</div>
        <div class="value neutral">${totalIgnored}</div>
      </div>
      ${
        feLinesTotal
          ? `
      <div class="card">
        <div class="label">Frontend Line Cov</div>
        <div class="value ${colorClass(feLinesTotal.pct)}">${feLinesTotal.pct}%</div>
      </div>`
          : ''
      }
      ${
        rsLinesTotal
          ? `
      <div class="card">
        <div class="label">Rust Line Cov</div>
        <div class="value ${colorClass(rsLinesTotal.percent)}">${Math.round(rsLinesTotal.percent * 10) / 10}%</div>
      </div>`
          : ''
      }
    </div>`;

  // ── crate results table ──
  const crateRows = tests
    .map((r) => {
      const status = r.failed === 0 ? 'ok' : 'fail';
      return `<tr>
      <td class="file-name">${r.crate}</td>
      <td><span class="badge ${status}">${status === 'ok' ? 'PASS' : 'FAIL'}</span></td>
      <td>${r.passed}</td>
      <td>${r.failed}</td>
      <td>${r.ignored}</td>
    </tr>`;
    })
    .join('');

  const crateSection = `
    <div class="section">
      <div class="section-header">Rust Tests by Crate</div>
      ${
        tests.length === 0
          ? `<div class="empty">No test results — run <code>yarn coverage</code> first</div>`
          : `<table>
          <thead><tr><th>Crate</th><th>Status</th><th>Passed</th><th>Failed</th><th>Ignored</th></tr></thead>
          <tbody>${crateRows}</tbody>
        </table>`
      }
    </div>`;

  // ── frontend coverage table ──
  const feRows = fe.rows
    .map(
      (r) => `<tr>
    <td class="file-name" title="${r.file}">${r.file}</td>
    <td>${bar(r.lines)}</td>
    <td>${bar(r.stmts)}</td>
    <td>${bar(r.funcs)}</td>
    <td>${bar(r.branches)}</td>
  </tr>`,
    )
    .join('');

  const totalRow = feLinesTotal
    ? `
    <tr style="font-weight:600">
      <td>TOTAL</td>
      <td>${bar(feLinesTotal.pct)}</td>
      <td>${bar(fe.totals!.statements.pct)}</td>
      <td>${bar(fe.totals!.functions.pct)}</td>
      <td>${bar(fe.totals!.branches.pct)}</td>
    </tr>`
    : '';

  const feSection = `
    <div class="section">
      <div class="section-header">Frontend Coverage (TypeScript)</div>
      ${
        fe.rows.length === 0
          ? `<div class="empty">No frontend coverage data — run <code>yarn coverage:frontend</code> first</div>`
          : `<table>
          <thead><tr><th>File</th><th>Lines</th><th>Statements</th><th>Functions</th><th>Branches</th></tr></thead>
          <tbody>${feRows}${totalRow}</tbody>
        </table>
        <div class="links"><a class="link-btn" href="frontend/index.html">Open detailed HTML report →</a></div>`
      }
    </div>`;

  // ── rust coverage table ──
  const rsRows = rs.rows
    .map(
      (r) => `<tr>
    <td class="file-name" title="${r.file}">${r.file}</td>
    <td>${bar(r.lines)}</td>
    <td>${bar(r.funcs)}</td>
    <td>${bar(r.regions)}</td>
  </tr>`,
    )
    .join('');

  const rsTotalRow = rsLinesTotal
    ? `
    <tr style="font-weight:600">
      <td>TOTAL</td>
      <td>${bar(Math.round(rsLinesTotal.percent * 10) / 10)}</td>
      <td>${bar(Math.round(rs.totals!.functions.percent * 10) / 10)}</td>
      <td>${bar(Math.round(rs.totals!.regions.percent * 10) / 10)}</td>
    </tr>`
    : '';

  const rsSection = `
    <div class="section">
      <div class="section-header">Rust Coverage</div>
      ${
        rs.rows.length === 0
          ? `<div class="empty">No Rust coverage data — run <code>yarn coverage:rust</code> first</div>`
          : `<table>
          <thead><tr><th>File</th><th>Lines</th><th>Functions</th><th>Regions</th></tr></thead>
          <tbody>${rsRows}${rsTotalRow}</tbody>
        </table>
        <div class="links"><a class="link-btn" href="rust/html/index.html">Open detailed HTML report →</a></div>`
      }
    </div>`;

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Rocket — Test Coverage Dashboard</title>
  <style>${CSS}</style>
</head>
<body>
  <h1>🚀 Rocket — Test Coverage Dashboard</h1>
  <p class="subtitle ts">Generated ${generatedAt.toLocaleString()}</p>
  ${summaryCards}
  ${crateSection}
  ${feSection}
  ${rsSection}
</body>
</html>`;
}

// ─── main ────────────────────────────────────────────────────────────────────

mkdirSync(COVERAGE_DIR, { recursive: true });

const fe = loadFrontendCoverage();
const rs = loadRustCoverage();
const tests = runCargoTests();

const html = generateHTML(fe, rs, tests, new Date());
const outPath = join(COVERAGE_DIR, 'index.html');
writeFileSync(outPath, html, 'utf8');

const totalPassed = tests.reduce((s, r) => s + r.passed, 0);
const totalFailed = tests.reduce((s, r) => s + r.failed, 0);
console.log(`\nDashboard written → ${outPath}`);
console.log(`Tests: ${totalPassed} passed, ${totalFailed} failed`);
if (fe.totals) console.log(`Frontend coverage: ${fe.totals.lines.pct}% lines`);
if (rs.totals)
  console.log(`Rust coverage: ${Math.round(rs.totals.lines.percent * 10) / 10}% lines`);
