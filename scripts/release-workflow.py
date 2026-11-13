"""Prepare reproducible release jobs, then publish or resume their exact bytes."""
import argparse
from datetime import datetime, timezone, timedelta
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
QUALITY_POLICY = 'release-quality-v2-bundled'
RUST_CHECKS = (
    ('rust-format', ('cargo', 'fmt', '--all', '--', '--check')),
    ('rust-lint', ('cargo', 'clippy', '--locked', '--all-targets', '--all-features', '--', '-D', 'warnings')),
    ('rust-tests', ('cargo', 'test', '--locked')),
)


def quality_commands(product):
    """Versioned quality policy for Hub and its bundled processor."""
    if product == 'hub':
        return [
            ('frontend-dependencies', ('npm', 'ci'), '.'),
            ('frontend-check', ('npm', 'run', 'check'), '.'),
            *((name, command, 'src-tauri') for name, command in RUST_CHECKS),
        ]
    if product == 'processor':
        return [(name, command, '.') for name, command in RUST_CHECKS]
    raise RuntimeError(f'未知源码质量检查目标：{product}')


def utc_timestamp():
    return datetime.now(timezone.utc).isoformat()


def verify_source_snapshot(product, source, commit, destination, environment):
    """Check precisely the archived tree that will be passed to the packager.

    Failure evidence stays in the preparation directory for diagnosis. No
    resumable job is created until every required gate and build has succeeded.
    """
    if not re.fullmatch(r'[0-9a-f]{40}', commit):
        raise RuntimeError('质量检查需要完整的来源提交。')
    snapshot = source.relative_to(destination).as_posix()
    archive = destination / f'{product}-source.zip'
    report_path = destination / f'verification-{product}.json'
    report = {
        'schema': 1, 'policy': QUALITY_POLICY, 'product': product,
        'source_commit': commit, 'snapshot': snapshot,
        'source_archive': {'file': archive.name, 'sha256': digest(archive)},
        'status': 'running', 'started_at': utc_timestamp(), 'commands': [],
    }
    write_json(report_path, report)
    for name, command, relative in quality_commands(product):
        working_directory = source if relative == '.' else source / relative
        step = {
            'id': name, 'command': list(command),
            'working_directory': working_directory.relative_to(destination).as_posix(),
            'source_commit': commit, 'started_at': utc_timestamp(), 'status': 'running',
        }
        report['commands'].append(step)
        write_json(report_path, report)
        print(f'质量检查 [{product}/{name}]：{" ".join(command)}', flush=True)
        try:
            run(command, working_directory, env=environment)
        except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
            step.update(status='failed', exit_code=getattr(error, 'returncode', None),
                        completed_at=utc_timestamp(), error=str(error))
            report.update(status='failed', completed_at=utc_timestamp())
            write_json(report_path, report)
            raise RuntimeError(f'{product} 质量检查 {name} 未通过，已停止打包；记录：{report_path}') from error
        step.update(status='passed', exit_code=0, completed_at=utc_timestamp())
        write_json(report_path, report)
    report.update(status='passed', completed_at=utc_timestamp())
    write_json(report_path, report)
    return report_path.name


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


def source_snapshot(repo, destination, name, commit):
    """Build tracked source only; old ignored generated permissions are not inputs."""
    archive = destination / f'{name}-source.zip'
    source = destination / f'source-{name}'
    run(['git', 'archive', '--format=zip', '--output', archive, commit], repo)
    source.mkdir()
    with zipfile.ZipFile(archive) as files:
        for item in files.infolist():
            target = (source / item.filename).resolve()
            if (not target.is_relative_to(source.resolve()) or '\\' in item.filename
                    or (item.external_attr >> 16) & 0o170000 == 0o120000):
                raise RuntimeError('源码快照包含不支持的路径或链接。')
        files.extractall(source)
    return source


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
        raise RuntimeError(f'{name} 含加工记录，不能作为纯净Mod 包发布。')
    report_name = ('generation-manifest.json' if (root / 'generation-manifest.json').is_file()
                   else 'enhancement-manifest.json')
    if not (root / report_name).is_file():
        raise RuntimeError(f'{name} 缺少成品来源清单。')
    report = read_json(root / report_name)
    enhanced = report_name == 'enhancement-manifest.json'
    producers = ({'d2r-litehub-plus-personal-builder', 'd2rhub-local-mod-builder'}
                 if enhanced else {'d2r-native-bundled-generator'})
    if (report.get('mod_name') != name or report.get('profile') != PROFILES[name]
            or report.get('producer') not in producers
            or (not enhanced and report.get('mode') != 'bundled_rebuild')
            or report.get('verified_output_integrity') is not True):
        raise RuntimeError(f'{name} 的生成来源或方案不符。')
    data_version = (root / f'{name}.mpq/data/global/dataversionbuild.txt').read_text(encoding='utf-8-sig').strip()
    if not data_version.isdigit() or str(report.get('game_data_version')) != data_version:
        raise RuntimeError(f'{name} 的实际游戏数据版本与生成记录不符。')
    if not (root / f'{name}.mpq/modinfo.json').is_file():
        raise RuntimeError(f'{name} 缺少 modinfo.json。')
    if enhanced:
        metadata = read_json(root / 'mod-version.json')
        if (metadata.get('mod_name') != name or metadata.get('mod_version') != report.get('mod_version')
                or str(metadata.get('game_data_version')) != data_version
                or read_json(root / f'{name}.mpq/modinfo.json').get('name') != name):
            raise RuntimeError(f'{name} 的成品版本或名称不符。')
        recorded = report.get('files')
        if not isinstance(recorded, dict) or not recorded:
            raise RuntimeError(f'{name} 缺少成品文件摘要。')
        actual = {f.relative_to(root / f'{name}.mpq').as_posix(): f
                  for f in files if f.is_relative_to(root / f'{name}.mpq')}
        # Game-generated BIN caches may be absent from the authored manifest.
        extra = {key for key, file in actual.items() if key not in recorded
                 and not (key.startswith('data/global/excel/') and file.suffix.lower() == '.bin'
                          and file.with_suffix('.txt').is_file())}
        if extra or set(recorded) - set(actual):
            raise RuntimeError(f'{name} 的成品文件列表不符。')
        for relative, expected in recorded.items():
            file = actual[relative]
            if (not isinstance(expected, dict) or expected.get('bytes') != file.stat().st_size
                    or expected.get('sha256') != digest(file)):
                raise RuntimeError(f'{name} 的成品文件摘要不符：{relative}')
    else:
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
                    if relative == report_name else file.read_bytes())
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


def software_release_description(source, version, commit, day=None):
    notes_path = Path(source) / 'docs/releases' / f'v{version}.md'
    if not notes_path.is_file():
        raise RuntimeError(f'缺少软件发行说明：{notes_path}')
    lines = notes_path.read_text(encoding='utf-8-sig').strip().splitlines()
    notes = '\n'.join(lines[1:]).strip() if lines and lines[0].startswith('# ') else '\n'.join(lines).strip()
    if not notes:
        raise RuntimeError(f'软件发行说明为空：{notes_path}')
    day = day or datetime.now(timezone(timedelta(hours=8))).date().isoformat()
    title = f'D2RHub v{version}'
    return {'release_name': title,
            'release_notes': f'# {title} · {day}\n\n{notes}\n\n来源提交：`{commit}`\n'}


def build_software(destination, processor_repo=None):
    commit = source_commit(ROOT)
    source = source_snapshot(ROOT, destination, 'hub', commit)
    version = read_json(source / 'package.json')['version']
    with (source / 'src-tauri/Cargo.toml').open('rb') as stream:
        rust_version = tomllib.load(stream)['package']['version']
    lock = read_json(source / 'package-lock.json')
    versions = [rust_version, read_json(source / 'src-tauri/tauri.conf.json')['version'],
                lock['version'], lock['packages']['']['version']]
    if not re.fullmatch(r'\d+\.\d+\.\d+', version) or any(v != version for v in versions):
        raise RuntimeError('Hub 版本号必须为正式版本，并在 npm、Cargo、Tauri 配置中一致。')
    description = software_release_description(source, version, commit)
    processor, processor_commit = build_processor(processor_repo or ROOT.parent / 'd2r-audio-mod', destination)
    bundled = source / 'src-tauri/processor/d2r-audio-mod.exe'
    bundled.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(processor['file'], bundled)
    processor_source = destination / 'source-processor'
    shutil.copyfile(processor_source / 'LICENSE', bundled.parent / 'processor-LICENSE.txt')
    shutil.copytree(processor_source / 'crates/stormlib-sys/LICENSES', bundled.parent / 'LICENSES')
    shutil.copyfile(processor_source / 'crates/stormlib-sys/THIRD_PARTY_NOTICES.md', bundled.parent / 'THIRD_PARTY_NOTICES.md')
    write_json(bundled.with_suffix('.json'), {'sha256': digest(bundled), 'source_commit': processor_commit})
    target = destination / 'build-hub'
    environment = os.environ.copy()
    environment['CARGO_TARGET_DIR'] = str(target)
    environment['D2RHUB_PROCESSOR_PREBUILT'] = '1'
    verify_source_snapshot('hub', source, commit, destination, environment)
    # npm consumes the first separator; Tauri forwards the second to Cargo.
    run(['npm', 'run', 'build:nsis', '--', '--', '--locked'], source, env=environment)
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
            'assets': [{'id': 'hub', 'version': version, 'file': str(path),
                        'release_tag': 'v' + version, 'source_commit': commit, **description}]}, commit


def build_processor(repo, destination):
    repo = Path(repo)
    commit = source_commit(repo)
    source = source_snapshot(repo, destination, 'processor', commit)
    with (source / 'Cargo.toml').open('rb') as stream:
        version = tomllib.load(stream)['package']['version']
    target = destination / 'build-processor'
    environment = os.environ.copy()
    environment['CARGO_TARGET_DIR'] = str(target)
    verify_source_snapshot('processor', source, commit, destination, environment)
    run(['cargo', 'build', '--locked', '--release', '--bin', 'd2r-audio-mod',
         '--target-dir', target], source, env=environment)
    executable = target / 'release/d2r-audio-mod.exe'
    actual = run([executable, '--version'], source, capture=True)
    if not processor_version_matches(actual, version):
        raise RuntimeError('加工器实际版本与 Cargo.toml 不符。')
    path = destination / f'd2r-audio-mod-{version}-windows-x64.exe'
    shutil.copyfile(executable, path)
    if source_commit(repo) != commit:
        raise RuntimeError('构建过程中加工器源码发生变化，请重新准备。')
    return {'version': version, 'file': str(path)}, commit


def processor_version_matches(output, version):
    match = re.fullmatch(r'd2r-audio-mod\s+(\S+)(?:\s+\(protocol v\d+\))?', output.strip())
    return match is not None and match.group(1) == version


def prepare(target, cfg):
    if target not in ('software', 'mods', 'all'):
        raise RuntimeError('加工器只随 Hub 安装包发布。')
    stamp = datetime.now(timezone.utc).strftime('%Y%m%d%H%M%S')
    folder = Path(cfg['output_root']) / (stamp + '-' + uuid.uuid4().hex[:8])
    folder.mkdir(parents=True, exist_ok=False)
    print(f'准备目录：{folder}', flush=True)
    job = {'schema': 2, 'target': target, 'sources': {}, 'specs': [], 'files': {}, 'verification': {}}
    if target in ('software', 'all'):
        spec, commit = build_software(folder, cfg.get('processor_repo'))
        job['sources']['processor_commit'] = read_json(folder / 'verification-processor.json')['source_commit']
        job['verification']['processor'] = 'verification-processor.json'
        job['sources']['hub_commit'] = commit
        job['verification']['hub'] = 'verification-hub.json'
        write_json(folder / 'software.json', spec)
        job['specs'].append('software.json')
    if target in ('mods', 'all'):
        catalog = read_json(ROOT / 'resources/mod-resources-v2.json')
        spec = {key: catalog[key] for key in ('kind', 'channel', 'hub_min', 'hub_max_exclusive', 'release_url')}
        spec['skip_unchanged'] = True
        spec['assets'] = []
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
    if job.get('schema') not in (1, 2) or not job.get('specs'):
        raise RuntimeError('无效的发布任务。')
    for name, identity in job['files'].items():
        path = folder / name
        if path.resolve().parent != folder or path.is_symlink():
            raise RuntimeError('发布任务包含非法文件路径。')
        if path.stat().st_size != identity['size'] or digest(path) != identity['sha256']:
            raise RuntimeError(f'准备后的文件已改变，禁止继续发布：{name}')
    required_verification = set()
    for name in job['specs']:
        if name not in job['files']:
            raise RuntimeError('发布配置没有完整性记录。')
        spec = read_json(folder / name)
        for asset in spec['assets']:
            path = Path(asset['file'])
            if path.resolve().parent != folder or path.name not in job['files']:
                raise RuntimeError('发布文件不在本次准备目录中。')
            if spec['kind'] == 'software':
                required_verification.update(('hub', 'processor'))
                source = job.get('sources', {}).get('hub_commit', '')
                if (not re.fullmatch(r'[0-9a-f]{40}', source)
                        or asset.get('source_commit', source) != source):
                    raise RuntimeError('软件发布配置与任务来源提交不一致。')
            elif asset.get('id') == 'processor':
                required_verification.add('processor')
    if job['schema'] == 2:
        verify_quality_evidence(folder, job, required_verification)
    return job


def verify_quality_evidence(folder, job, required):
    """Resume validates saved evidence, never reruns commands or reads source trees."""
    if not isinstance(job.get('verification'), dict) or set(job['verification']) != required:
        raise RuntimeError('发布任务缺少完整的质量检查记录。')
    for product in sorted(required):
        name = f'verification-{product}.json'
        if job['verification'][product] != name or name not in job['files']:
            raise RuntimeError('质量检查记录没有完整性保护。')
        report = read_json(folder / name)
        commit = job.get('sources', {}).get(f'{product}_commit', '')
        archive = f'{product}-source.zip'
        expected_commands = quality_commands(product)
        if (report.get('schema') != 1 or report.get('policy') != QUALITY_POLICY
                or report.get('product') != product or report.get('status') != 'passed'
                or not re.fullmatch(r'[0-9a-f]{40}', commit)
                or report.get('source_commit') != commit
                or report.get('snapshot') != f'source-{product}'
                or archive not in job['files']
                or report.get('source_archive') != {'file': archive, 'sha256': job['files'][archive]['sha256']}
                or len(report.get('commands', [])) != len(expected_commands)):
            raise RuntimeError('质量检查结果与本次发布源码不一致。')
        for step, (identifier, command, relative) in zip(report['commands'], expected_commands):
            working_directory = f'source-{product}' + (f'/{relative}' if relative != '.' else '')
            if (step.get('id') != identifier or step.get('command') != list(command)
                    or step.get('working_directory') != working_directory
                    or step.get('source_commit') != commit or step.get('status') != 'passed'
                    or step.get('exit_code') != 0
                    or not step.get('started_at') or not step.get('completed_at')):
                raise RuntimeError('质量检查命令未完整通过，禁止继续发布。')


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
        if spec['kind'] == 'software':
            # Older prepared jobs already recorded hub_commit in job.json. Use
            # it without rebuilding or modifying their integrity-checked files.
            source = job.get('sources', {}).get('hub_commit')
            for asset in spec['assets']:
                if asset.get('source_commit', source) != source:
                    raise RuntimeError('软件发布配置与任务来源提交不一致。')
                asset['source_commit'] = source
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
    from release_platforms import credentials, Platform, is_hub_version_correction
    settings, token = credentials(Path(cfg['publisher_config']))
    version = tuple(map(int, asset['version'].split('.')))
    # Only promote the software tag. Never make a resource/index tag latest.
    github = Platform('github', settings['github_repo'], '')
    latest = github.call('GET', '/releases/latest', missing=True)
    if latest:
        current = latest['tag_name'].removeprefix('v')
        if (re.fullmatch(r'\d+\.\d+\.\d+', current) and tuple(map(int, current.split('.'))) > version
                and not is_hub_version_correction(current, asset['version'])):
            raise RuntimeError('已有更高正式软件版本，拒绝降低 latest。')
    source = asset.get('source_commit', '')
    if not re.fullmatch(r'[0-9a-f]{40}', source):
        raise RuntimeError('正式发布需要完整的软件来源提交。')
    github.release(asset['release_tag'], create=False, source_commit=source)
    for platform in (Platform('gitee', settings['gitee_repo'], token), github):
        source = asset.get('source_commit') if platform.name == 'github' else None
        release = platform.release(asset['release_tag'], create=False, source_commit=source)
        if not release:
            raise RuntimeError('缺少已验证的软件 Release。')
        body = {'tag_name': release['tag_name'], 'name': asset.get('release_name', release['name']),
                'body': asset.get('release_notes', release['body']), 'prerelease': False}
        if platform.name == 'github':
            body['make_latest'] = 'true'
        platform.call('PATCH', f"/releases/{release['id']}", json=body)
    print(f"正式版已提升：{asset['version']}")


def main():
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')
    parser = argparse.ArgumentParser(description='D2RHub 一键准备、双端发布与失败补传（Python 3.10+）')
    parser.add_argument('--target', choices=['software', 'mods', 'all'])
    parser.add_argument('--publish', action='store_true', help='准备完成后上传；默认只准备')
    parser.add_argument('--promote', action='store_true', help='双端验证成功后同时提升软件正式版')
    parser.add_argument('--resume', type=Path, help='复用已有任务目录，禁止重建文件')
    parser.add_argument('--config', type=Path, default=DEFAULT_CONFIG)
    args = parser.parse_args()
    if args.target and args.resume:
        parser.error('--target 与 --resume 不能同时使用')
    if args.promote and not args.publish:
        parser.error('--promote 需要 --publish')
    if args.promote and args.target == 'mods':
        parser.error('--promote 只适用于 software、all 或包含软件的补传任务')
    cfg = configuration(args.config.resolve())
    if not args.target and not args.resume:
        print('\nD2RHub 发布工作流\n1. Hub 软件（含加工器）\n3. 三个Mod\n4. 全部\n5. 补传已有任务\n6. 修改本机路径配置\n0. 退出')
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
            targets = {'1': 'software', '3': 'mods', '4': 'all'}
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
