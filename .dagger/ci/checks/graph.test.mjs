import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { matchesGlob } from 'node:path';
import test from 'node:test';
import { runAff } from '../output/Effect.Aff/index.js';
import { Left } from '../output/Data.Either/index.js';
import { connectWith } from '../output/Dagger/index.js';
import * as Query from '../output/Dagger.Query/index.js';
import * as Base from '../output/CI.Base/index.js';
import * as Graph from '../output/CI.Graph/index.js';
import * as Build from '../output/CI.Build/index.js';
import * as Firmware from '../output/CI.Firmware/index.js';
import * as Runner from '../output/CI.Runner/index.js';

// Construct graphs only. A network call is a test failure, never an engine run.
globalThis.fetch = () => { throw new Error('These tests must never contact an engine'); };
const client = connectWith({ port: 1, token: 'test', timeoutMs: 1 })();
const container = Query.container(client);
const run = aff => new Promise((resolve, reject) => runAff(result => () =>
  result instanceof Left ? reject(result.value0) : resolve(result.value0))(aff)());
const argument = (step, name) => run(step.args.find(arg => arg.name === name).value);

function excluded(path, patterns) {
  const pieces = path.split('/');
  return patterns.some(pattern => pieces.some((_, i) => matchesGlob(pieces.slice(0, i + 1).join('/'), pattern)));
}

test('source filtering retains firmware/target while excluding Cargo output', async () => {
  const graph = Base.sourceTree(client)(Query.directory(client));
  const filters = await argument(graph.steps.find(s => s.name === 'withDirectory'), 'exclude');
  for (const path of [
    'firmware/target/hosted/filesystem-app.h',
    'firmware/target/hosted/headless/headless.make',
    'firmware/target/hosted/headless/system-headless.c',
    'firmware/target/arm/system-arm.h',
  ]) assert.equal(excluded(path, filters), false, path);
  for (const path of ['target/release/rockbox', 'desktop/target/release/rockbox-desktop', 'bindings/ruby/target/a.o'])
    assert.equal(excluded(path, filters), true, path);
  // Exercise the entire tracked firmware subtree, not just a short allowlist.
  const files = readdirSync(new URL('../../../firmware/target/', import.meta.url), { recursive: true });
  for (const file of files) assert.equal(excluded(`firmware/target/${file}`, filters), false, file);
});

test('web builds are separate direct executable calls', async () => {
  const graph = Build.web(container);
  const commands = await Promise.all(graph.steps.filter(s => s.name === 'withExec').map(s => argument(s, 'args')));
  assert.deepEqual(commands, [
    ['deno', 'install'], ['deno', 'task', 'build'],
    ['bun', 'install', '--frozen-lockfile'], ['bun', 'run', 'build'],
  ]);
});

test('archive names and stdout redirection stay literal argv values', async () => {
  const graph = Graph.archive('release with spaces.tar.gz')('/stage/bin')(['rockbox', 'rockboxd'])(container);
  const commands = graph.steps.filter(s => s.name === 'withExec');
  assert.deepEqual(await argument(commands[0], 'args'), ['tar', '-czf', '/out/release with spaces.tar.gz', '-C', '/stage/bin', 'rockbox', 'rockboxd']);
  assert.equal(await argument(commands[1], 'redirectStdout'), 'release with spaces.tar.gz.sha256');
});

test('codec inventory and release inputs retain build contracts', () => {
  assert.equal(Firmware.codecNames.length, 29);
  assert.ok(Firmware.codecNames.includes('opus'));
  assert.ok(Firmware.oggSymbols.includes('ogg_stream_pagein'));
  for (const version of ['1.2.3', 'v2026.10.26', 'latest']) assert.equal(Runner.validVersion(version), true);
  for (const version of ['', '../escape', '1.0\nother', '$(command)', '"quoted"']) assert.equal(Runner.validVersion(version), false);
});

test('CI modules contain no shell command dispatch or script delegation', () => {
  const dir = new URL('../src/CI/', import.meta.url);
  for (const name of readdirSync(dir).filter(name => name.endsWith('.purs'))) {
    const text = readFileSync(new URL(name, dir), 'utf8');
    assert.doesNotMatch(text, /\[\s*"(?:bash|sh|zsh)"/, name);
    assert.doesNotMatch(text, /dockerBuildWith|\/ci\/scripts\/|GITHUB_ENV|GITHUB_OUTPUT/, name);
  }
});
