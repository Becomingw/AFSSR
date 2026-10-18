// Offline check: every converted request body dumped by crates/aether-ai/formats/tests/go_real_clients.rs
// is run through the same strict OpenCode Go validators the e2e upstream uses.
//   node go_validate.mjs <dir>
import fs from 'node:fs';
import * as go from './go_mode.mjs';

const dir = process.argv[2];
const allowTraining = true;
let bad = 0;
const reasons = new Map();
for (const file of fs.readdirSync(dir).sort()) {
  const record = JSON.parse(fs.readFileSync(`${dir}/${file}`, 'utf8'));
  const label = file.replace(/\.json$/, '');
  let problem = null;
  if (!record.ok) problem = `conversion refused: ${record.error.slice(0, 200)}`;
  else {
    const protocol = go.protocolOf(record.model);
    const refusal = go.validate(protocol, record.model, record.body, { allowTraining });
    if (refusal) problem = `upstream would refuse: ${refusal.body?.error?.message ?? JSON.stringify(refusal.body)}`.slice(0, 260);
  }
  if (problem) {
    bad++;
    console.log(`FAIL ${label}: ${problem}`);
    const key = problem.replace(/\d+/g, 'N').slice(0, 120);
    reasons.set(key, (reasons.get(key) ?? 0) + 1);
  }
}
console.log(`\n${fs.readdirSync(dir).length - bad}/${fs.readdirSync(dir).length} converted bodies accepted`);
for (const [reason, count] of [...reasons].sort((a, b) => b[1] - a[1])) console.log(`  ${count}  ${reason}`);
process.exit(bad ? 1 : 0);
