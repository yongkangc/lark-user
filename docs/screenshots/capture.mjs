import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const { chromium } = await import(pathToFileURL(process.env.PLAYWRIGHT_MODULE).href);
const binary = resolve(process.argv[2]);
const repo = resolve(import.meta.dirname, '../..');
const store = join(mkdtempSync(join(tmpdir(), 'lark-user-demo-')), 'store');
const run = (args, input) => JSON.parse(execFileSync(binary, ['--store-dir', store, '--json', ...args], { input, encoding: 'utf8' }));
run(['profile', 'configure', '--web-url', 'https://tenant.example.test', '--account-id', 'synthetic-account-001', '--tenant-id', 'synthetic-tenant-001', '--retention-days', '3650']);
run(['archive', 'import', '--file', '-'], readFileSync(join(repo, 'tests/fixtures/normalized.jsonl')));
const search = run(['messages', 'search', 'generic report', '--local']);
const context = run(['context', '--chat', 'synthetic-chat-001', '--local', '--max-bytes', '4096']);
const views = [
  { file: 'local-search', title: 'Find the signal.', subtitle: 'Search authorized local exports. Keep the source.', command: 'lark-user messages search "generic report" --local', data: { source: search.source, matches: search.items.map(({ message_id, text }) => ({ message_id, text })), completeness: search.completeness } },
  { file: 'agent-context', title: 'Context with receipts.', subtitle: 'Bounded. Attributed. Ready for your agent.', command: 'lark-user context --chat synthetic-chat-001 --local --max-bytes 4096', data: { source: context.source, byte_limit: context.byte_limit, omitted_for_budget: context.omitted_for_budget, messages: context.messages.map(({ trust, source, message }) => ({ trust, message_id: source.message_id, text: message.text })) } },
];
const escape = (s) => s.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1120 }, deviceScaleFactor: 1 });
  for (const view of views) {
    const output = JSON.stringify(view.data, null, 2);
    writeFileSync(join(import.meta.dirname, `${view.file}.json`), JSON.stringify({ command: view.command, selected_fields: view.data }, null, 2) + '\n');
    await page.setContent(`<!doctype html><meta charset="utf-8"><style>
      *{box-sizing:border-box}body{margin:0;background:#0b1221;color:#e7eefc;font-family:Arial,sans-serif;padding:62px 70px}
      .brand{color:#76e2cd;font-size:20px;letter-spacing:3px;font-weight:700}h1{font-size:62px;margin:18px 0 12px;letter-spacing:-2px}
      p{color:#a7b9d3;font-size:25px;margin:0 0 35px}.terminal{border:1px solid #334763;border-radius:18px;background:#111e33;overflow:hidden;box-shadow:0 25px 60px #0005}
      .bar{padding:18px 26px;background:#1d2c43;color:#a7b9d3;font-size:16px;display:flex;justify-content:space-between}
      pre{font-family:'Liberation Mono',monospace;white-space:pre-wrap;overflow-wrap:anywhere;font-size:19px;line-height:1.48;margin:0;padding:26px;color:#c7d6ec}
      .command{color:#76e2cd;padding-bottom:0}.foot{display:flex;justify-content:space-between;color:#91a4c3;margin-top:26px;font-size:17px}
      </style><div class="brand">LARK-USER / LOCAL FIRST</div><h1>${view.title}</h1><p>${view.subtitle}</p>
      <div class="terminal"><div class="bar"><span>CLI output · selected fields</span><span>Synthetic demo</span></div>
      <pre class="command">$ ${escape(view.command)}</pre><pre>${escape(output)}</pre></div>
      <div class="foot"><span>No credentials. No live requests.</span><span>International Lark protocol remains unverified.</span></div>`);
    await page.screenshot({ path: join(import.meta.dirname, `${view.file}.png`), fullPage: true });
  }
} finally {
  await browser.close();
}
