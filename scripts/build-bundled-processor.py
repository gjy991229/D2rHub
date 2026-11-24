"""Build the processor into the resources included by every Hub installer."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
DESTINATION = ROOT / 'src-tauri' / 'processor' / 'd2r-audio-mod.exe'


def build():
    if os.environ.get('D2RHUB_PROCESSOR_PREBUILT') == '1':
        manifest = json.loads(DESTINATION.with_suffix('.json').read_text(encoding='utf-8'))
        if hashlib.sha256(DESTINATION.read_bytes()).hexdigest() != manifest['sha256']:
            raise RuntimeError('Prepared bundled processor integrity check failed')
        return
    repo = Path(os.environ.get('D2RHUB_PROCESSOR_REPO', str(ROOT.parent / 'd2r-audio-mod')))
    target = repo / 'target'
    subprocess.run(['cargo', 'build', '--locked', '--release', '--bin', 'd2r-audio-mod',
                    '--target-dir', str(target)], cwd=repo, check=True)
    binary = target / 'release' / 'd2r-audio-mod.exe'
    DESTINATION.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(binary, DESTINATION)
    shutil.copyfile(repo / 'LICENSE', DESTINATION.parent / 'processor-LICENSE.txt')
    shutil.copytree(repo / 'crates/stormlib-sys/LICENSES', DESTINATION.parent / 'LICENSES', dirs_exist_ok=True)
    shutil.copyfile(repo / 'crates/stormlib-sys/THIRD_PARTY_NOTICES.md', DESTINATION.parent / 'THIRD_PARTY_NOTICES.md')
    DESTINATION.with_suffix('.json').write_text(json.dumps({
        'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'capabilities': json.loads(subprocess.check_output(
            [str(binary), 'capabilities', '--json'], encoding='utf-8'))['capabilities']
    }), encoding='utf-8')


if __name__ == '__main__':
    build()
