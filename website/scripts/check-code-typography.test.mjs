import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { findCodeTypographyViolations } from './check-code-typography.mjs';

const checkerPath = fileURLToPath(new URL('./check-code-typography.mjs', import.meta.url));

test('finds prohibited punctuation in fenced, inline, HTML code, and preformatted code', () => {
  const source = [
    'Prosa con raya \u2013 y comillas \u201ccurvas\u201d.',
    '~~~sh',
    'cargo test \u2013example server',
    '~~~',
    'Texto con `redirect_back(\u201c/profile\u201d)` y <code>rel=\u201cexternal\u201d</code>.',
    '<pre>',
    'status: \u201cvalid\u201d',
    '</pre>',
  ].join('\n');

  const violations = findCodeTypographyViolations(source);

  assert.deepEqual(
    violations.map(({ line, character }) => [line, character]),
    [
      [3, '\u2013'],
      [5, '\u201c'],
      [5, '\u201d'],
      [5, '\u201c'],
      [5, '\u201d'],
      [7, '\u201c'],
      [7, '\u201d'],
    ],
  );
});

test('allows typographic punctuation in prose and ASCII punctuation in code', () => {
  const source = [
    'La prosa puede usar raya \u2013 y comillas \u201ccurvas\u201d.',
    '```sh',
    'cargo test --example server',
    '```',
    'El comando `redirect_back("/profile")` usa comillas ASCII.',
  ].join('\n');

  assert.deepEqual(findCodeTypographyViolations(source), []);
});

test('ignores unmatched and escaped inline backticks', () => {
  const source = [
    'Un delimitador sin pareja ` no forma un span.',
    'Un backtick escapado \\` tampoco abre un span con \u201cprosa\u201d.',
  ].join('\n');

  assert.deepEqual(findCodeTypographyViolations(source), []);
});

test('does not pair inline delimiters across paragraphs', () => {
  const source = [
    '` sin cierre en este párrafo',
    '',
    '` prosa tipográfica \u201cpermitida\u201d sin cierre.',
  ].join('\n');

  assert.deepEqual(findCodeTypographyViolations(source), []);
});

test('CLI fails and reports a code violation', async (context) => {
  const directory = await mkdtemp(join(tmpdir(), 'rustflow-docs-invalid-'));
  context.after(() => rm(directory, { recursive: true, force: true }));
  await writeFile(join(directory, 'invalid.md'), '~~~sh\ncargo test \u2013example app\n~~~\n');

  const result = spawnSync(process.execPath, [checkerPath, directory], { encoding: 'utf8' });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /invalid\.md:2:/);
});

test('CLI accepts prose typography and ASCII code', async (context) => {
  const directory = await mkdtemp(join(tmpdir(), 'rustflow-docs-valid-'));
  context.after(() => rm(directory, { recursive: true, force: true }));
  await writeFile(join(directory, 'valid.mdx'), 'Prosa \u201cpermitida\u201d.\n`cargo test --example app`\n');

  const result = spawnSync(process.execPath, [checkerPath, directory], { encoding: 'utf8' });

  assert.equal(result.status, 0);
  assert.match(result.stdout, /passed for 1 Markdown\/MDX files/);
});
