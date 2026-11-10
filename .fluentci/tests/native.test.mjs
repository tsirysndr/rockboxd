import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import * as Engine from '../ci/output/Native.Engine/index.js';

function files(root) {
  return readdirSync(root, { withFileTypes: true }).flatMap(entry => {
    const path = join(root, entry.name);
    return entry.isDirectory() ? files(path) : [path];
  });
}
test('glob matching supports root and nested paths without crossing a single-star boundary', () => {
  const matches = (pattern, value) => new RegExp(Engine.globRegex(pattern)).test(value);
  assert.ok(matches('**/*.rpm', 'app.rpm'));
  assert.ok(matches('**/*.rpm', 'x86_64/app.rpm'));
  assert.ok(matches('bindings/**/target', 'bindings/ruby/native/target'));
  assert.ok(!matches('*.deb', 'nested/app.deb'));
  assert.ok(matches('a+b?.so', 'a+b1.so'));
  assert.ok(!matches('a+b?.so', 'aaab1.so'));
});
test('component failures are synchronous and survive cleanup', () => {
  assert.throws(() => Engine.runJob(() => { throw new Error('build failed'); })(), /build failed/);
});
test('FluentCI source has no Dagger SDK, executable, or shell-program adapters', () => {
  const root = new URL('../ci/src/', import.meta.url);
  for (const path of files(root.pathname).filter(path => path.endsWith('.purs'))) {
    const source = readFileSync(path, 'utf8');
    assert.doesNotMatch(source, /import Dagger|\[\s*"dagger"|\[\s*"(?:sh|bash)",\s*"-c"/, path);
  }
});
