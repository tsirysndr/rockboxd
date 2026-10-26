#!/usr/bin/env python3
"""Check the CI wiring without starting Dagger or building Rockbox."""
from pathlib import Path
import json
import re
import subprocess
import tomllib
import yaml

root = Path(__file__).resolve().parent.parent
ci = root / '.dagger/ci'
for path in [root / 'mise.toml', root / 'dagger.toml', *root.glob('.dagger/**/dagger-module.toml')]:
    data = tomllib.loads(path.read_text())
    if 'engineVersion' in data:
        assert data['engineVersion'] == 'v1.0.0-beta.21', path
assert tomllib.loads((root / 'mise.toml').read_text())['tools']['http:dagger']['version'] == '1.0.0-beta.21'
assert not (root / 'dagger.json').exists()
assert not (root / '.fluentci').exists()

for path in [root / '.dagger/run', root / '.dagger/ci.sh', *ci.glob('scripts/*.sh')]:
    subprocess.run(['bash', '-n', str(path)], check=True)
    assert '${{' not in path.read_text(), path

job_names = re.findall(r'-> "([a-z0-9_-]+)"', (ci / 'src/CI/Job.purs').read_text().split('toolchain ::')[0])
wrapper = (root / '.dagger/ci.sh').read_text()
for name in job_names:
    assert (ci / 'scripts' / f'{name}.sh').is_file(), name
    assert re.search(r'^  ' + re.escape(name) + r'\)', wrapper, re.M), name
functions = set(re.findall(r'Module.function "([^"]+)"', (ci / 'src/Main.purs').read_text()))
cli_functions = {re.sub(r'[A-Z]', lambda m: '-' + m[0].lower(), name) for name in functions}
assert set(re.findall(r'function=([a-z0-9_-]+)', wrapper)) <= cli_functions

count = 0
for path in (root / '.github/workflows').glob('*.yml*'):
    text = path.read_text()
    assert 'setup-fluentci' not in text and 'fluentci run' not in text, path
    doc = yaml.safe_load(text)
    assert 'on' in doc or True in doc, path
    for job in doc['jobs'].values():
        steps = job.get('steps', [])
        for index, step in enumerate(steps):
            if '.dagger/ci.sh' in step.get('run', '') or 'dagger api call' in step.get('run', ''):
                assert any(s.get('uses') == './.github/actions/setup-dagger' for s in steps[:index]), path
                assert any(s.get('uses', '').startswith('actions/checkout@') for s in steps[:index]), path
                if 'CI_JOB' in step.get('env', {}):
                    assert step['env']['CI_JOB'] in job_names, path
                count += 1
# Licenses and the generated client must survive fresh module builds.
for path in ['LICENSE', 'schema/LICENSE']:
    assert (ci / 'dagger-sdk' / path).is_file(), path
    assert (root / '.dagger/sdk/purescript' / path).is_file(), path
assert (ci / 'dagger-sdk/src/Dagger/Container.purs').is_file()
assert json.loads((ci / 'package.json').read_text())['type'] == 'module'
print(f'Checked {len(job_names)} jobs and {count} workflow calls; no engine started.')
