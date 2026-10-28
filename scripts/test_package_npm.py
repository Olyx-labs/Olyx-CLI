"""Check npm manifests, binary contents and rejection of invalid native inputs."""
import importlib.util
import json
from pathlib import Path
import shutil
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
def module(name, file):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / file)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result
npm = module('npm_packager', 'package-npm.py')
native = module('native_packager', 'package-release.py')

@unittest.skipUnless(shutil.which('npm'), 'npm packaging checks require Node.js/npm')
class NpmPackaging(unittest.TestCase):
    def test_all_platform_packages_preserve_verified_binary_and_have_no_hooks(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'fixture'
            binary.write_bytes(b'native executable fixture')
            for target, (system, cpu) in npm.TARGETS.items():
                archive = native.package(binary, 'v0.2.0', target, root / 'archives')
                packed = npm.prepare(root / 'npm', archive)
                with tarfile.open(packed) as bundle:
                    manifest = json.load(bundle.extractfile('package/package.json'))
                    name = 'oy'
                    self.assertEqual(bundle.extractfile(f'package/bin/{name}').read(), binary.read_bytes())
                    self.assertEqual({member.name for member in bundle.getmembers()},
                                     {'package/package.json', 'package/LICENSE', 'package/NOTICE', f'package/bin/{name}'})
                self.assertEqual(manifest['name'], f'olyx-cli-{system}-{cpu}')
                self.assertEqual(manifest['os'], [system])
                self.assertEqual(manifest['cpu'], [cpu])
                self.assertNotIn('scripts', manifest)
                self.assertEqual(manifest['version'], '0.2.0')

    def test_launcher_dependencies_match_supported_binaries(self):
        with tempfile.TemporaryDirectory() as directory:
            packed = npm.prepare(Path(directory))
            with tarfile.open(packed) as bundle:
                manifest = json.load(bundle.extractfile('package/package.json'))
                self.assertTrue(bundle.getmember('package/bin/oy.cjs').mode & 0o111)
                self.assertNotIn('scripts', manifest)
                self.assertEqual(manifest['bin'], {'oy': 'bin/oy.cjs'})
                self.assertEqual(manifest['optionalDependencies'], {
                    f'olyx-cli-{system}-{cpu}': manifest['version'] for system, cpu in npm.TARGETS.values()})

    def test_bad_checksum_cannot_be_repackaged(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'fixture'; binary.write_bytes(b'fixture')
            archive = native.package(binary, 'v0.2.0', 'aarch64-apple-darwin', root)
            archive.write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
                npm.prepare(root / 'npm', archive)
            self.assertEqual(list((root / 'npm').glob('*.tgz')), [])

    def test_unsupported_target_or_version_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            for name in ['olyx-v0.2.0-x86_64-apple-darwin.tar.gz', 'olyx-v9.9.9-aarch64-apple-darwin.tar.gz']:
                with self.assertRaisesRegex(ValueError, 'target/version'):
                    npm.prepare(Path(directory), Path(name))

if __name__ == '__main__': unittest.main()
