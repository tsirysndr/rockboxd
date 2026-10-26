import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import * as Format from '../output/CI.Format/index.js';

const cargo = (args, cwd) => spawnSync('cargo', args, { cwd, encoding: 'utf8', timeout: 30000 });

test('formatter skips auto-enrolled submodule packages but still rejects owned formatting errors', () => {
  const root = mkdtempSync(join(tmpdir(), 'rockbox-format-'));
  try {
    writeFileSync(join(root, 'Cargo.toml'), '[workspace]\nmembers = ["app", "other"]\ndefault-members = ["app"]\nresolver = "2"\n');
    const crates = [
      ['app', 'format-app', 'pub fn ready() {}\n', '[dependencies]\ndeno_generated = { path = "../deno/generated" }\n'],
      ['other', 'netstream', 'pub fn owned() {}\n', ''],
      ['deno/generated', 'deno_generated', 'pub fn generated( ){ }\n', ''],
    ];
    for (const [path, name, source, extra] of crates) {
      mkdirSync(join(root, path, 'src'), { recursive: true });
      writeFileSync(join(root, path, 'Cargo.toml'), `[package]\nname = "${name}"\nversion = "0.1.0"\nedition = "2021"\n${extra}`);
      writeFileSync(join(root, path, 'src/lib.rs'), source);
    }
    const result = cargo(['metadata', '--no-deps', '--format-version', '1', '--offline'], root);
    assert.equal(result.status, 0, result.stderr);
    const metadata = JSON.parse(result.stdout);
    const dependency = metadata.packages.find(p => p.name === 'deno_generated');
    assert.ok(metadata.workspace_members.includes(dependency.id), 'fixture must reproduce Cargo auto-enrollment');
    const names = Format.packages(metadata);
    assert.deepEqual(names.sort(), ['format-app', 'netstream']);
    assert.notEqual(cargo(['fmt', '--all', '--check'], root).status, 0, 'old check must fail on generated source');
    const [executable, ...args] = Format.command(names);
    assert.equal(executable, 'cargo');
    const clean = cargo(args, root);
    assert.equal(clean.status, 0, clean.stdout + clean.stderr);
    writeFileSync(join(root, 'other/src/lib.rs'), 'pub fn owned( ){ }\n');
    const dirty = cargo(args, root);
    assert.notEqual(dirty.status, 0, 'owned formatting errors must still fail');
    assert.match(dirty.stdout, /other[\/]src[\/]lib.rs/);
    assert.equal(readFileSync(join(root, 'deno/generated/src/lib.rs'), 'utf8'), 'pub fn generated( ){ }\n');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('selection uses membership and manifest boundaries, not package-name prefixes', () => {
  const metadata = {
    workspace_root: '/src',
    workspace_members: ['own', 'deno', 'rmpc', 'external', 'neighbor'],
    packages: [
      { id: 'own', name: 'netstream', manifest_path: '/src/crates/netstream/Cargo.toml' },
      { id: 'deno', name: 'rockbox-in-deno', manifest_path: '/src/deno/ext/rb/Cargo.toml' },
      { id: 'rmpc', name: 'rmpc', manifest_path: '/src/rmpc/Cargo.toml' },
      { id: 'external', name: 'external', manifest_path: '/elsewhere/Cargo.toml' },
      { id: 'neighbor', name: 'deno-tools', manifest_path: '/src/deno-tools/Cargo.toml' },
      { id: 'nonmember', name: 'unused', manifest_path: '/src/crates/unused/Cargo.toml' },
    ],
  };
  assert.deepEqual(Format.packages(metadata), ['netstream', 'deno-tools']);
});
