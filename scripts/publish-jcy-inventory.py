"""Prepare the pinned JCY preset and optionally publish the two resource mirrors."""

import argparse
import json
from pathlib import Path
import shutil
from subprocess import TimeoutExpired
from tempfile import TemporaryDirectory

from release_platforms import CONFIG, Platform, anonymous_verify, credentials, sha256

ROOT = Path(__file__).resolve().parents[1]
TAG = 'jcy-inventory-v1'
NAME = f'{TAG}.preset.gz'
REPOSITORIES = {'github': 'gjy991229/D2rHub', 'gitee': 'garyi7e/d2-rhub_-resource'}


def check_asset(path, asset):
    if path.stat().st_size != asset['size'] or sha256(path) != asset['sha256']:
        raise RuntimeError('JCY preset size or SHA-256 does not match the pinned manifest')


def prepare(source, output, asset):
    check_asset(source, asset)
    output.mkdir(parents=True, exist_ok=True)
    target = output / NAME
    if target.exists():
        check_asset(target, asset)
    else:
        with TemporaryDirectory(prefix='prepare-', dir=output) as temporary:
            staged = Path(temporary) / NAME
            shutil.copyfile(source, staged)
            check_asset(staged, asset)
            staged.replace(target)
    return target


def publish(path, asset, config):
    check_asset(path, asset)
    mirrors = {mirror['platform']: mirror['url'] for mirror in asset['mirrors']}
    expected_urls = {
        platform: f'https://{platform}.com/{repo}/releases/download/{TAG}/{NAME}'
        for platform, repo in REPOSITORIES.items()
    }
    if mirrors != expected_urls:
        raise RuntimeError('Pinned mirror URLs do not match the publication destinations')
    cfg, token = credentials(config)
    if any(cfg[f'{platform}_repo'] != repo for platform, repo in REPOSITORIES.items()):
        raise RuntimeError('Publisher config does not match the pinned repositories')

    verified = {}
    errors = []
    for platform, repo in REPOSITORIES.items():
        try:
            publisher = Platform(platform, repo, token if platform == 'gitee' else '')
            release = publisher.release(TAG)
            if not any(entry['name'] == NAME for entry in publisher.assets(release)):
                publisher.upload(release, path)
            url = mirrors[platform]
            anonymous_verify(url, asset['size'], asset['sha256'])
            verified[platform] = {'url': url, 'size': asset['size'], 'sha256': asset['sha256']}
            print(f'{platform}: anonymous download verified', flush=True)
        except (RuntimeError, OSError, TimeoutExpired) as exc:
            errors.append(f'{platform}: {exc}')
    return verified, errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--config', type=Path, default=CONFIG)
    parser.add_argument('--publish', action='store_true')
    args = parser.parse_args()
    asset = json.loads((ROOT / f'resources/{TAG}.json').read_text(encoding='utf-8'))
    path = prepare(args.source, args.output, asset)
    print(f'Prepared {path}', flush=True)
    if args.publish:
        verified, errors = publish(path, asset, args.config)
        report = {'verified': verified, 'errors': errors}
        (args.output / 'verification.json').write_text(
            json.dumps(report, indent=2) + '\n', encoding='utf-8',
        )
        if errors:
            raise SystemExit('\n'.join(errors))


if __name__ == '__main__':
    main()
