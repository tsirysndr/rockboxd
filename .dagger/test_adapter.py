#!/usr/bin/env python3
"""Exercise the host adapter with a fake CLI; never start a Dagger engine."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent

class AdapterTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.fake = Path(self.tmp.name) / 'dagger'
        self.fake.write_text('#!/usr/bin/env python3\nimport json,sys\nprint(json.dumps(sys.argv[1:]))\n')
        self.fake.chmod(0o755)

    def call(self, job, **overrides):
        env = {k: v for k, v in os.environ.items() if k not in [
            'TAG', 'SOURCE', 'OUTPUT', 'PLATFORM', 'ANDROID_SDK_ROOT',
            'BINDING_VERSION', 'ARTIFACTS', 'ABI', 'FEATURES', 'ARCHITECTURES']}
        env.update(PATH=self.tmp.name + os.pathsep + env['PATH'], **overrides)
        return subprocess.run(['bash', str(ROOT / '.dagger/ci.sh'), job],
                              env=env, text=True, capture_output=True)

    def args(self, result):
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def test_arm_release_and_export(self):
        args = self.args(self.call('linux-aarch64-release', TAG='2026.10.26', OUTPUT='release output'))
        self.assertEqual(args[:5], ['api', 'call', '-m', '.dagger/ci', 'linux-release'])
        self.assertEqual(args[args.index('--platform') + 1], 'linux/arm64')
        self.assertEqual(args[args.index('--tag') + 1], '2026.10.26')
        self.assertEqual(args[-3:], ['export', '--path', 'release output'])

    def test_android_arguments_preserve_spaces(self):
        args = self.args(self.call('android-apk', ANDROID_SDK_ROOT='/tmp/Android SDK',
                                   FEATURES='embedded-daemon', ARCHITECTURES='arm64-v8a,x86_64'))
        self.assertEqual(args[args.index('--android-sdk') + 1], '/tmp/Android SDK')
        self.assertEqual(args[args.index('--architectures') + 1], 'arm64-v8a,x86_64')
        self.assertNotEqual(self.call('android-apk').returncode, 0)

    def test_binding_artifacts_are_required_and_forwarded(self):
        self.assertNotEqual(self.call('package-python').returncode, 0)
        args = self.args(self.call('package-python', BINDING_VERSION='1.2.3', ARTIFACTS='native artifacts'))
        self.assertEqual(args[4], 'python')
        self.assertEqual(args[args.index('--artifacts') + 1], 'native artifacts')
        self.assertEqual(args[args.index('--version') + 1], '1.2.3')

    def test_unknown_job_never_calls_cli(self):
        result = self.call('../../arbitrary-script')
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, '')

if __name__ == '__main__':
    unittest.main()
