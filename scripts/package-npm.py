"""Prepare npm packages from a verified native archive; never publish anything."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {
    'aarch64-apple-darwin': ('darwin', 'arm64'),
    'aarch64-unknown-linux-musl': ('linux', 'arm64'),
    'x86_64-unknown-linux-musl': ('linux', 'x64'),
}


def pack(stage, output):
    npm = shutil.which('npm')
    if not npm:
        raise ValueError('npm is required to prepare npm release packages')
    result = subprocess.run([npm, 'pack', '--ignore-scripts', '--json', '--pack-destination', str(output)],
                            cwd=stage, check=True, capture_output=True, encoding='utf-8')
    return output / json.loads(result.stdout)[0]['filename']


def prepare(output, archive=None):
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    launcher = json.loads((ROOT / 'npm/package.json').read_text())
    with tempfile.TemporaryDirectory(prefix='olyx-npm-') as directory:
        stage = Path(directory)
        if archive:
            archive = archive.resolve()
            names = {f'olyx-v{launcher["version"]}{suffix}-{target}.{extension}': target
                     for target in TARGETS for suffix in ['', '-rehearsal']
                     for extension in ['tar.gz']}
            target = names.get(archive.name)
            if target is None:
                raise ValueError('archive target/version does not match the npm launcher')
            checksum = Path(str(archive) + '.sha256').read_text().split()
            if len(checksum) != 2 or checksum[1] != archive.name or hashlib.sha256(archive.read_bytes()).hexdigest() != checksum[0]:
                raise ValueError('archive checksum mismatch')
            system, cpu = TARGETS[target]
            executable = 'oy'
            root = archive.name[:-7]
            member = f'{root}/{executable}'
            with tarfile.open(archive) as bundle:
                entry = bundle.getmember(member)
                if not entry.isfile():
                    raise ValueError('native executable must be a regular file')
                payload = bundle.extractfile(entry).read()
            manifest = {'name': f'olyx-cli-{system}-{cpu}', 'version': launcher['version'],
                        'description': f'Olyx native executable for {system} {cpu}', 'license': 'Apache-2.0',
                        'repository': launcher['repository'], 'os': [system], 'cpu': [cpu],
                        'files': ['bin', 'LICENSE', 'NOTICE'], 'publishConfig': {'access': 'public'}}
            (stage / 'bin').mkdir()
            binary = stage / 'bin' / executable
            binary.write_bytes(payload)
            binary.chmod(0o755)
        else:
            manifest = launcher
            shutil.copytree(ROOT / 'npm/bin', stage / 'bin')
            (stage / 'bin/oy.cjs').chmod(0o755)
            shutil.copyfile(ROOT / 'npm/README.md', stage / 'README.md')
        for name in ['LICENSE', 'NOTICE']:
            shutil.copyfile(ROOT / name, stage / name)
        (stage / 'package.json').write_text(json.dumps(manifest, indent=2) + '\n')
        return pack(stage, output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, help='Native release archive; omit to pack the launcher')
    parser.add_argument('--output', type=Path, default=Path('dist/npm'))
    args = parser.parse_args()
    print(prepare(args.output, args.archive))
