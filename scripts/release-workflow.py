"""Prepare reproducible release jobs, then publish or resume their exact bytes."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib
import uuid
import zipfile

ROOT = Path(__file__).resolve().parent.parent
LOCAL = Path(os.environ.get('LOCALAPPDATA', str(Path.home()))) / 'D2RHub-Publisher'
DEFAULT_CONFIG = LOCAL / 'workflow.json'
PROFILES = {'LiteHub': 'main', 'BoHub': 'filler', 'NullHub': 'min'}
MARKERS = {'d2rhub-mod-manifest.json', 'audio-telemetry-manifest.json'}


def read_json(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    temporary.replace(path)


def digest(path):
    hasher = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            hasher.update(block)
    return hasher.hexdigest()


def run(args, cwd=ROOT, env=None, capture=False):
    executable = shutil.which(str(args[0]))
    if not executable:
        raise RuntimeError(f'缺少命令：{args[0]}。请先安装开发依赖。')
    result = subprocess.run([executable, *map(str, args[1:])], cwd=cwd, env=env,
                            text=True, encoding='utf-8', errors='replace',
                            stdout=subprocess.PIPE if capture else None, check=True)
    return result.stdout.strip() if capture else None


def source_commit(root):
    # Include untracked files: publishing undocumented local changes is too ambiguous.
    if run(['git', 'status', '--porcelain'], root, capture=True):
        raise RuntimeError(f'源码有未提交改动，请先提交再构建发布：{root}')
    return run(['git', 'rev-parse', 'HEAD'], root, capture=True)


def configuration(path):
    if not path.exists():
        common = Path(run(['git', 'rev-parse', '--path-format=absolute', '--git-common-dir'], capture=True))
        write_json(path, {
            'processor_repo': str(common.parent.parent / 'd2r-audio-mod'),
            'mods_root': r'C:\Diablo II Resurrected\mods',
            'output_root': str(ROOT / 'artifacts' / 'releases'),
            'publisher_config': str(LOCAL / 'config.json'),
        })
        print(f'已生成本机路径配置：{path}')
    cfg = read_json(path)
    for key in ('processor_repo', 'mods_root', 'output_root', 'publisher_config'):
        value = Path(os.path.expandvars(cfg[key])).expanduser()
        cfg[key] = str((path.parent / value).resolve() if not value.is_absolute() else value.resolve())
    return cfg


def safe_files(root):
    def visit(directory):
        if directory.is_symlink() or getattr(directory.lstat(), 'st_file_attributes', 0) & 0x400:
            raise RuntimeError(f'不允许目录链接或重解析点：{directory}')
        for child in sorted(directory.iterdir()):
            info = child.lstat()
            if child.is_symlink() or getattr(info, 'st_file_attributes', 0) & 0x400:
                raise RuntimeError(f'不允许链接或重解析点：{child}')
            if child.is_dir():
                yield from visit(child)
            elif child.is_file():
                yield child
            else:
                raise RuntimeError(f'不支持的文件类型：{child}')
    yield from visit(root)


def package_mod(root, name, target):
    files = list(safe_files(root))
    before = {str(f): (f.stat().st_size, f.stat().st_mtime_ns) for f in files}
    if any(f.name.lower() in MARKERS for f in files):
        raise RuntimeError(f'{name} 含加工记录，不能作为纯净官方包发布。')
    report = read_json(root / 'generation-manifest.json')
    if (report.get('mod_name') != name or report.get('profile') != PROFILES[name]
            or report.get('producer') != 'd2r-native-bundled-generator'
            or report.get('mode') != 'bundled_rebuild'
            or report.get('verified_output_integrity') is not True):
        raise RuntimeError(f'{name} 的官方生成来源或方案不符。')
    data_version = (root / f'{name}.mpq/data/global/dataversionbuild.txt').read_text(encoding='utf-8-sig').strip()
    if not data_version.isdigit() or str(report.get('game_data_version')) != data_version:
        raise RuntimeError(f'{name} 的实际游戏数据版本与生成记录不符。')
    if not (root / f'{name}.mpq/modinfo.json').is_file():
        raise RuntimeError(f'{name} 缺少 modinfo.json。')
    report['mod_directory'] = name
    # Strip machine paths and runtime-generated caches. Fixed ZIP timestamps and
    # permissions make the same source bytes produce the same artifact.
    hashes = {}
    with zipfile.ZipFile(target, 'x', zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for file in files:
            relative = file.relative_to(root).as_posix()
            if relative == '.d2rhub-resource.json':
                continue
            if (relative.startswith(f'{name}.mpq/data/global/excel/')
                    and file.suffix.lower() == '.bin' and file.with_suffix('.txt').is_file()):
                continue
            data = (json.dumps(report, ensure_ascii=False, sort_keys=True, indent=2).encode('utf-8')
                    if relative == 'generation-manifest.json' else file.read_bytes())
            hashes[relative] = hashlib.sha256(data).hexdigest()
            info = zipfile.ZipInfo(f'{name}/{relative}', date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            archive.writestr(info, data, compress_type=zipfile.ZIP_DEFLATED, compresslevel=6)
    after = {str(f): (f.stat().st_size, f.stat().st_mtime_ns) for f in safe_files(root)}
    if before != after:
        raise RuntimeError(f'{name} 在打包期间被修改，请关闭游戏或编辑工具后重试。')
    return data_version, hashes


def build_software(destination):
    commit = source_commit(ROOT)
    version = read_json(ROOT / 'package.json')['version']
    with (ROOT / 'src-tauri/Cargo.toml').open('rb') as stream:
        rust_version = tomllib.load(stream)['package']['version']
    lock = read_json(ROOT / 'package-lock.json')
    versions = [rust_version, read_json(ROOT / 'src-tauri/tauri.conf.json')['version'],
                lock['version'], lock['packages']['']['version']]
    if not re.fullmatch(r'\d+\.\d+\.\d+', version) or any(v != version for v in versions):
        raise RuntimeError('Hub 版本号必须为正式版本，并在 npm、Cargo、Tauri 配置中一致。')
    target = destination / 'build-hub'
    environment = os.environ.copy()
    environment['CARGO_TARGET_DIR'] = str(target)
    run(['npm', 'ci'])
    run(['npm', 'run', 'build:nsis'], env=environment)
    candidates = list((target / 'release/bundle/nsis').glob('*-setup.exe'))
    if len(candidates) != 1:
        raise RuntimeError('未得到唯一的 NSIS 安装包。')
    from release_platforms import windows_file_version
    if windows_file_version(candidates[0]) != version:
        raise RuntimeError('安装包内嵌版本不符。')
    path = destination / f'D2RHub_{version}_x64-setup.exe'
    shutil.copyfile(candidates[0], path)
    if source_commit(ROOT) != commit:
        raise RuntimeError('构建过程中 Hub 源码发生变化，请重新准备。')
    return {'kind': 'software', 'product': 'D2RHub', 'platform': 'windows-x86_64',
            'assets': [{'id': 'hub', 'version': version, 'file': str(path), 'release_tag': 'v' + version}]}, commit


def build_processor(repo, destination):
    repo = Path(repo)
    commit = source_commit(repo)
    with (repo / 'Cargo.toml').open('rb') as stream:
        version = tomllib.load(stream)['package']['version']
    target = destination / 'build-processor'
    run(['cargo', 'build', '--locked', '--release', '--bin', 'd2r-audio-mod',
         '--target-dir', target], repo)
    executable = target / 'release/d2r-audio-mod.exe'
    actual = run([executable, '--version'], repo, capture=True)
    if not processor_version_matches(actual, version):
        raise RuntimeError('加工器实际版本与 Cargo.toml 不符。')
    path = destination / f'd2r-audio-mod-{version}-windows-x64.exe'
    shutil.copyfile(executable, path)
    if source_commit(repo) != commit:
        raise RuntimeError('构建过程中加工器源码发生变化，请重新准备。')
    return {'id': 'processor', 'version': version, 'file': str(path),
            'release_tag': 'processor-v' + version, 'mod_name': None,
            'profile': None, 'game_data_version': None}, commit


def processor_version_matches(output, version):
    match = re.fullmatch(r'd2r-audio-mod\s+(\S+)(?:\s+\(protocol v\d+\))?', output.strip())
    return match is not None and match.group(1) == version


def prepare(target, cfg):
    stamp = datetime.now(timezone.utc).strftime('%Y%m%d%H%M%S')
    folder = Path(cfg['output_root']) / (stamp + '-' + uuid.uuid4().hex[:8])
    folder.mkdir(parents=True, exist_ok=False)
    print(f'准备目录：{folder}', flush=True)
    job = {'schema': 1, 'target': target, 'sources': {}, 'specs': [], 'files': {}}
    if target in ('software', 'all'):
        spec, commit = build_software(folder)
        job['sources']['hub_commit'] = commit
        write_json(folder / 'software.json', spec)
        job['specs'].append('software.json')
    if target in ('processor', 'mods', 'all'):
        catalog = read_json(ROOT / 'resources/mod-resources-v2.json')
        spec = {key: catalog[key] for key in ('kind', 'channel', 'hub_min', 'hub_max_exclusive', 'release_url')}
        spec['skip_unchanged'] = True
        spec['assets'] = []
        if target in ('processor', 'all'):
            asset, commit = build_processor(cfg['processor_repo'], folder)
            spec['assets'].append(asset)
            job['sources']['processor_commit'] = commit
        if target in ('mods', 'all'):
            tag = 'mod-resources-' + stamp
            for name, profile in PROFILES.items():
                path = folder / f'{name}-{tag}.zip'
                game_version, hashes = package_mod(Path(cfg['mods_root']) / name, name, path)
                write_json(folder / f'{name}-files.json', hashes)
                job['sources'][name] = {'path': str(Path(cfg['mods_root']) / name),
                                        'file_manifest': f'{name}-files.json'}
                spec['assets'].append({'id': name, 'version': tag, 'file': str(path),
                                       'release_tag': tag, 'mod_name': name, 'profile': profile,
                                       'game_data_version': game_version})
            if len({a['game_data_version'] for a in spec['assets'] if a['id'] in PROFILES}) != 1:
                raise RuntimeError('三个 Mod 的游戏数据版本不一致。')
        write_json(folder / 'resources.json', spec)
        job['specs'].append('resources.json')
    for file in folder.iterdir():
        if file.is_file():
            job['files'][file.name] = {'size': file.stat().st_size, 'sha256': digest(file)}
    write_json(folder / 'job.json', job)
    describe(folder, job)
    return folder


def verify_job(folder):
    folder = Path(folder).resolve()
    job = read_json(folder / 'job.json')
    if job.get('schema') != 1 or not job.get('specs'):
        raise RuntimeError('无效的发布任务。')
    for name, identity in job['files'].items():
        path = folder / name
        if path.resolve().parent != folder or path.is_symlink():
            raise RuntimeError('发布任务包含非法文件路径。')
        if path.stat().st_size != identity['size'] or digest(path) != identity['sha256']:
            raise RuntimeError(f'准备后的文件已改变，禁止继续发布：{name}')
    for name in job['specs']:
        if name not in job['files']:
            raise RuntimeError('发布配置没有完整性记录。')
        for asset in read_json(folder / name)['assets']:
            path = Path(asset['file'])
            if path.resolve().parent != folder or path.name not in job['files']:
                raise RuntimeError('发布文件不在本次准备目录中。')
    return job


def describe(folder, job):
    print('\n已准备以下文件（尚未上传）：')
    for name in job['specs']:
        spec = read_json(folder / name)
        if spec['kind'] == 'resources':
            print(f"兼容 Hub：{spec['hub_min']} 至 {spec['hub_max_exclusive']}（不含上界），通道：{spec['channel']}")
        for asset in spec['assets']:
            print(f"  {asset['id']}  {asset['version']}  {Path(asset['file']).stat().st_size:,} bytes")
    print(f'来源和摘要：{folder / "job.json"}')
    print(f'补传命令：.\\release.ps1 -Resume "{folder}" -Publish')


def load_publisher():
    spec = importlib.util.spec_from_file_location('download_publisher', ROOT / 'scripts/publish-downloads.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def publish_job(folder, cfg, promote=False):
    folder = Path(folder).resolve()
    job = verify_job(folder)
    if promote and not any(read_json(folder / name)['kind'] == 'software' for name in job['specs']):
        raise RuntimeError('本次任务没有软件安装包，不能提升软件正式版。')
    publisher = load_publisher()
    # Every retry reuses staged bytes but gets a new index revision/output folder.
    revision = int(datetime.now(timezone.utc).strftime('%Y%m%d%H%M%S'))
    previous_attempts = [int(p.name) for p in (folder / 'attempts').glob('*') if p.name.isdigit()]
    revision = max([revision, *[n + 1 for n in previous_attempts]])
    attempt = folder / 'attempts' / str(revision)
    complete = True
    software = None
    for name in job['specs']:
        spec = read_json(folder / name)
        output = attempt / spec['kind']
        publisher.publish(spec, revision, output, Path(cfg['publisher_config']))
        report = read_json(output / 'publish-report.json')
        complete &= set(report['published']) == {'github', 'gitee'}
        if spec['kind'] == 'software':
            software = spec['assets'][0]
    if promote:
        if not complete:
            raise RuntimeError('镜像未完成，暂不提升正式版；补传时加 -Promote 重试。')
        if software is None:
            raise RuntimeError('本次任务没有软件安装包，不能提升软件正式版。')
        promote_software(software, cfg)
    print(f'发布报告：{attempt}')
    print('双端发布完成。' if complete else '镜像未完成；请使用上方补传命令重试。')
    return 0 if complete else 2


def promote_software(asset, cfg):
    from release_platforms import credentials, Platform
    settings, token = credentials(Path(cfg['publisher_config']))
    version = tuple(map(int, asset['version'].split('.')))
    # Only promote the software tag. Never make a resource/index tag latest.
    github = Platform('github', settings['github_repo'], '')
    latest = github.call('GET', '/releases/latest', missing=True)
    if latest:
        current = latest['tag_name'].removeprefix('v')
        if re.fullmatch(r'\d+\.\d+\.\d+', current) and tuple(map(int, current.split('.'))) > version:
            raise RuntimeError('已有更高正式软件版本，拒绝降低 latest。')
    for platform in (Platform('gitee', settings['gitee_repo'], token), github):
        release = platform.release(asset['release_tag'], create=False)
        if not release:
            raise RuntimeError('缺少已验证的软件 Release。')
        body = {'tag_name': release['tag_name'], 'name': release['name'],
                'body': release['body'], 'prerelease': False}
        if platform.name == 'github':
            body['make_latest'] = 'true'
        platform.call('PATCH', f"/releases/{release['id']}", json=body)
    print(f"正式版已提升：{asset['version']}")


def main():
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')
    parser = argparse.ArgumentParser(description='D2RHub 一键准备、双端发布与失败补传（Python 3.10+）')
    parser.add_argument('--target', choices=['software', 'processor', 'mods', 'all'])
    parser.add_argument('--publish', action='store_true', help='准备完成后上传；默认只准备')
    parser.add_argument('--promote', action='store_true', help='双端验证成功后同时提升软件正式版')
    parser.add_argument('--resume', type=Path, help='复用已有任务目录，禁止重建文件')
    parser.add_argument('--config', type=Path, default=DEFAULT_CONFIG)
    args = parser.parse_args()
    if args.target and args.resume:
        parser.error('--target 与 --resume 不能同时使用')
    if args.promote and not args.publish:
        parser.error('--promote 需要 --publish')
    if args.promote and args.target in ('mods', 'processor'):
        parser.error('--promote 只适用于 software、all 或包含软件的补传任务')
    cfg = configuration(args.config.resolve())
    if not args.target and not args.resume:
        print('\nD2RHub 发布工作流\n1. Hub 软件\n2. Mod 加工器\n3. 三个官方 Mod\n4. 全部\n5. 补传已有任务\n6. 修改本机路径配置\n0. 退出')
        choice = input('选择：').strip()
        if choice == '0':
            return 0
        if choice == '6':
            for key in ('processor_repo', 'mods_root', 'output_root'):
                value = input(f'{key} [{cfg[key]}]：').strip().strip('"')
                if value:
                    cfg[key] = value
            write_json(args.config.resolve(), cfg)
            print('路径已保存，请重新运行。')
            return 0
        if choice == '5':
            folder = Path(input('任务目录：').strip().strip('"')).resolve()
            describe(folder, verify_job(folder))
        else:
            targets = {'1': 'software', '2': 'processor', '3': 'mods', '4': 'all'}
            if choice not in targets:
                raise RuntimeError('无效选择。')
            folder = prepare(targets[choice], cfg)
        print('Mod 生成记录不能证明此后没有手工修改；请确认准备目录中的文件是预期发布成品。')
        action = input('输入 p 发布；输入 s 发布并提升软件正式版；直接回车仅保留文件：').strip().lower()
        return publish_job(folder, cfg, action == 's') if action in ('p', 's') else 0
    folder = args.resume.resolve() if args.resume else prepare(args.target, cfg)
    if args.resume and not args.publish:
        describe(folder, verify_job(folder))
    return publish_job(folder, cfg, args.promote) if args.publish else 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (RuntimeError, OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f'发布已停止：{error}', file=sys.stderr)
        raise SystemExit(1)
