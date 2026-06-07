#!/usr/bin/env node
// Render every docs/diagrams/*.dot to a PNG next to its source (Graphviz + cairo,
// 220 DPI). The .dot files are the source of truth; the PNGs are committed for
// in-browser viewing on GitHub.
import { spawnSync } from 'node:child_process';
import { readdirSync } from 'node:fs';
import { join, basename } from 'node:path';

const repoRoot = process.cwd();
const diagramDir = join(repoRoot, 'docs', 'diagrams');

const probe = spawnSync('dot', ['-V'], { stdio: 'ignore' });
if (probe.error || probe.status !== 0) {
  console.error('Graphviz `dot` not found on PATH. Install with: brew install graphviz');
  process.exit(1);
}

const dotFiles = readdirSync(diagramDir)
  .filter((file) => file.endsWith('.dot'))
  .sort();

if (dotFiles.length === 0) {
  console.error('No .dot files found in docs/diagrams.');
  process.exit(1);
}

let rendered = 0;
const failures = [];

for (const file of dotFiles) {
  const source = join(diagramDir, file);
  const target = source.replace(/\.dot$/, '.png');

  const result = spawnSync('dot', ['-Tpng:cairo', source, '-Gdpi=220', '-o', target], {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  if (result.error) {
    failures.push(`${file}: ${result.error.message}`);
    continue;
  }
  if (result.status !== 0) {
    failures.push(`${file}: dot exited ${result.status}\n${result.stderr.trim()}`);
    continue;
  }

  rendered += 1;
  console.log(`rendered ${file} -> ${basename(target)}`);
}

if (failures.length > 0) {
  console.error('\nRender failed for:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`\n${rendered} diagram(s) rendered.`);
