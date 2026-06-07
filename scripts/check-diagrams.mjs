#!/usr/bin/env node
// Verify each docs/diagrams/*.dot renders cleanly and its committed PNG exists.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const repoRoot = process.cwd();
const diagramDir = join(repoRoot, 'docs', 'diagrams');

// Graphviz is required to render PNGs from .dot sources. CI installs it before
// `npm run check`; local machines without `dot` skip with a clear message.
const probe = spawnSync('dot', ['-V'], { stdio: 'ignore' });
if (probe.error || probe.status !== 0) {
  console.log('Diagram check skipped: Graphviz `dot` not available on PATH.');
  process.exit(0);
}

const tempDir = mkdtempSync(join(tmpdir(), 'geno1-diagrams-'));

const dotFiles = readdirSync(diagramDir)
  .filter((file) => file.endsWith('.dot'))
  .sort();

if (dotFiles.length === 0) {
  console.error('No .dot files found in docs/diagrams.');
  process.exit(1);
}

const failures = [];

// Verify each .dot renders cleanly and the committed PNG exists. PNGs are not
// byte-compared: Graphviz + libcairo emit different bytes across versions, which
// would produce stale-PNG false positives on every push. The .dot sources are the
// source of truth; PNGs are for in-browser viewing.
try {
  for (const file of dotFiles) {
    const source = join(diagramDir, file);
    const expectedPng = source.replace(/\.dot$/, '.png');
    const renderedPng = join(tempDir, file.replace(/\.dot$/, '.png'));

    if (!existsSync(expectedPng)) {
      failures.push(`${file}: missing committed PNG next to .dot source`);
      continue;
    }

    const result = spawnSync('dot', ['-Tpng:cairo', source, '-Gdpi=220', '-o', renderedPng], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    if (result.error) {
      failures.push(`${file}: could not run Graphviz dot (${result.error.message})`);
      continue;
    }
    if (result.status !== 0) {
      failures.push(`${file}: dot exited ${result.status}\n${result.stderr.trim()}`);
      continue;
    }
  }
} finally {
  rmSync(tempDir, { recursive: true, force: true });
}

if (failures.length > 0) {
  console.error('Diagram check failed:');
  for (const failure of failures) console.error(`- ${failure}`);
  console.error('\nRender locally with: npm run diagrams');
  process.exit(1);
}

console.log(`Diagram check passed (${dotFiles.length} diagrams render cleanly).`);
