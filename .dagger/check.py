#!/usr/bin/env python3
"""Validate CI contracts without starting Dagger or building Rockbox."""
from pathlib import Path
import json
import re
import tomllib
import yaml

root = Path(__file__).resolve().parent.parent
ci = root / '.dagger/ci'
for path in [root / 'mise.toml', root / 'dagger.toml', *root.glob('.dagger/**/dagger-module.toml')]:
    data = tomllib.loads(path.read_text())
    if 'engineVersion' in data:
        assert data['engineVersion'] == 'v1.0.0-beta.21', path
assert tomllib.loads((root / 'mise.toml').read_text())['tools']['http:dagger']['version'] == '1.0.0-beta.21'
for path in ['dagger.json', '.fluentci', '.dagger/ci.sh', '.dagger/run', '.dagger/ci/scripts']:
    assert not (root / path).exists(), path

functions = set(re.findall(r'Module.function "([^"]+)"', (ci / 'src/Main.purs').read_text()))
cli_functions = {re.sub(r'[A-Z]', lambda m: '-' + m[0].lower(), name) for name in functions}
count = 0
for path in (root / '.github/workflows').glob('*.yml*'):
    text = path.read_text()
    assert 'setup-fluentci' not in text and 'fluentci run' not in text, path
    assert '.dagger/ci.sh' not in text and '.dagger/run' not in text, path
    doc = yaml.safe_load(text)
    assert 'on' in doc or True in doc, path
    for job in doc['jobs'].values():
        steps = job.get('steps', [])
        for index, step in enumerate(steps):
            for function in re.findall(r'dagger api call -m \.dagger/ci ([a-z-]+)', step.get('run', '')):
                assert function in cli_functions, (path, function)
                assert any(s.get('uses') == './.github/actions/setup-dagger' for s in steps[:index]), path
                assert any(s.get('uses', '').startswith('actions/checkout@') for s in steps[:index]), path
                count += 1
for path in ['LICENSE', 'schema/LICENSE']:
    assert (ci / 'dagger-sdk' / path).is_file(), path
    assert (root / '.dagger/sdk/purescript' / path).is_file(), path
assert (ci / 'dagger-sdk/src/Dagger/Container.purs').is_file()
assert json.loads((ci / 'package.json').read_text())['type'] == 'module'
print(f'Checked {len(functions)} typed functions and {count} direct workflow calls; no engine started.')

# Standalone modules must retain their shared inputs and workspace scopes.
workspace = tomllib.loads((root / 'dagger.toml').read_text())
expected = {
    'linux-packaging': {'deb', 'rpm'},
    'archive': {'tarGz', 'sha256'},
    'gemfury': {'upload'},
    'github-assets': {'upload'},
}
for name, exports in expected.items():
    module = root / '.dagger/modules' / name
    config = tomllib.loads((module / 'dagger-module.toml').read_text())
    assert config['name'] == name, module
    assert config['runtime']['source'] == '../../sdk/purescript', module
    assert {'../../lib/**', '../../ci/dagger-sdk/**'} <= set(config['include']), module
    assert workspace['sdks']['purescript']['scopes'][f'.dagger/modules/{name}']['name'] == name
    actual = set(re.findall(r'Module.function "([^"]+)"', (module / 'src/Main.purs').read_text()))
    assert actual == exports, (module, actual)
    lock = json.loads((module / 'spago.lock').read_text())
    assert 'release-' + name in lock['workspace']['packages'], module
print(f'Checked {len(expected)} standalone release modules and their shared source wiring.')

for path in (root / '.github/workflows').glob('*.yml'):
    for job in yaml.safe_load(path.read_text())['jobs'].values():
        steps = job.get('steps', [])
        for index, step in enumerate(steps):
            for module, function in re.findall(r'dagger api call -m \.dagger/modules/([a-z-]+) ([a-z][a-z0-9-]*)', step.get('run', '')):
                exports = {re.sub(r'[A-Z]', lambda m: '-' + m[0].lower(), name) for name in expected[module]}
                assert function in exports, (path, module, function)
                assert any(s.get('uses') == './.github/actions/setup-dagger' for s in steps[:index]), path
