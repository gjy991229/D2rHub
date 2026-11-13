import importlib.util
import json
from pathlib import Path
import tempfile
import sys
import shutil
import subprocess
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location('workflow', Path(__file__).with_name('release-workflow.py'))
workflow = importlib.util.module_from_spec(spec)
spec.loader.exec_module(workflow)


class WorkflowTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.mod = self.root / 'LiteHub'
        excel = self.mod / 'LiteHub.mpq/data/global/excel'
        excel.mkdir(parents=True)
        (excel / 'skills.txt').write_text('original', encoding='utf-8')
        (excel / 'skills.bin').write_bytes(b'game cache')
        (excel / 'standalone.bin').write_bytes(b'keep')
        (self.mod / 'LiteHub.mpq/modinfo.json').write_text('{}')
        (self.mod / 'LiteHub.mpq/data/global/dataversionbuild.txt').write_text('93854')
        workflow.write_json(self.mod / 'generation-manifest.json', {
            'mod_name': 'LiteHub', 'profile': 'main', 'producer': 'd2r-native-bundled-generator',
            'mode': 'bundled_rebuild', 'verified_output_integrity': True,
            'mod_directory': 'C:/private/path', 'game_data_version': '93854',
        })

    def tearDown(self):
        self.temp.cleanup()

    def test_reproducible_package_excludes_only_derived_cache_and_normalizes_path(self):
        first, second = self.root / 'one.zip', self.root / 'two.zip'
        workflow.package_mod(self.mod, 'LiteHub', first)
        workflow.package_mod(self.mod, 'LiteHub', second)
        self.assertEqual(first.read_bytes(), second.read_bytes())
        with zipfile.ZipFile(first) as archive:
            names = archive.namelist()
            self.assertNotIn('LiteHub/LiteHub.mpq/data/global/excel/skills.bin', names)
            self.assertIn('LiteHub/LiteHub.mpq/data/global/excel/standalone.bin', names)
            self.assertEqual(json.loads(archive.read('LiteHub/generation-manifest.json'))['mod_directory'], 'LiteHub')
        self.assertEqual(workflow.read_json(self.mod / 'generation-manifest.json')['mod_directory'], 'C:/private/path')

    def test_processed_mod_rejected(self):
        (self.mod / 'audio-telemetry-manifest.json').write_text('{}')
        with self.assertRaisesRegex(RuntimeError, '加工记录'):
            workflow.package_mod(self.mod, 'LiteHub', self.root / 'bad.zip')

    def test_actual_game_version_must_match_report(self):
        (self.mod / 'LiteHub.mpq/data/global/dataversionbuild.txt').write_text('99999')
        with self.assertRaisesRegex(RuntimeError, '游戏数据版本'):
            workflow.package_mod(self.mod, 'LiteHub', self.root / 'bad.zip')

    def test_unknown_source_rejected(self):
        report = workflow.read_json(self.mod / 'generation-manifest.json')
        report['producer'] = 'unknown'
        workflow.write_json(self.mod / 'generation-manifest.json', report)
        with self.assertRaisesRegex(RuntimeError, '来源'):
            workflow.package_mod(self.mod, 'LiteHub', self.root / 'bad.zip')

    def enhanced_mod(self):
        (self.mod / 'generation-manifest.json').unlink()
        workflow.write_json(self.mod / 'LiteHub.mpq/modinfo.json', {'name': 'LiteHub'})
        metadata = {'mod_name': 'LiteHub', 'mod_version': '2026.09.28.3', 'game_data_version': '93854'}
        workflow.write_json(self.mod / 'mod-version.json', metadata)
        mpq = self.mod / 'LiteHub.mpq'
        workflow.write_json(self.mod / 'enhancement-manifest.json', {
            **metadata, 'producer': 'd2rhub-local-mod-builder', 'profile': 'main',
            'verified_output_integrity': True, 'runtime_verified': False,
            'files': {file.relative_to(mpq).as_posix(): {
                'bytes': file.stat().st_size, 'sha256': workflow.digest(file)}
                for file in mpq.rglob('*') if file.is_file() and file.name != 'skills.bin'},
        })

    def test_enhanced_product_is_verified_and_preserves_runtime_status(self):
        self.enhanced_mod()
        first, second = self.root / 'one.zip', self.root / 'two.zip'
        workflow.package_mod(self.mod, 'LiteHub', first)
        workflow.package_mod(self.mod, 'LiteHub', second)
        self.assertEqual(first.read_bytes(), second.read_bytes())
        with zipfile.ZipFile(first) as archive:
            self.assertFalse(json.loads(archive.read('LiteHub/enhancement-manifest.json'))['runtime_verified'])
            self.assertNotIn('LiteHub/LiteHub.mpq/data/global/excel/skills.bin', archive.namelist())

    def test_enhanced_product_rejects_changed_missing_and_extra_files(self):
        self.enhanced_mod()
        for change in ('changed', 'missing', 'extra'):
            with self.subTest(change=change):
                file = self.mod / 'LiteHub.mpq/data/global/excel/skills.txt'
                original = file.read_bytes()
                extra = file.with_name('unexpected.txt')
                if change == 'changed':
                    file.write_bytes(b'tampered')
                elif change == 'missing':
                    file.unlink()
                else:
                    extra.write_bytes(b'extra')
                with self.assertRaisesRegex(RuntimeError, '成品文件'):
                    workflow.package_mod(self.mod, 'LiteHub', self.root / 'bad.zip')
                file.write_bytes(original)
                extra.unlink(missing_ok=True)

    def test_enhanced_product_rejects_wrong_identity(self):
        self.enhanced_mod()
        workflow.write_json(self.mod / 'mod-version.json', {'mod_name': 'BoHub'})
        with self.assertRaisesRegex(RuntimeError, '版本或名称'):
            workflow.package_mod(self.mod, 'LiteHub', self.root / 'bad.zip')

    def job(self):
        file = self.root / 'processor.exe'
        file.write_bytes(b'original')
        workflow.write_json(self.root / 'resources.json', {
            'kind': 'resources', 'assets': [{'id': 'processor', 'file': str(file)}]})
        job = {'schema': 1, 'specs': ['resources.json'], 'files': {
            p.name: {'size': p.stat().st_size, 'sha256': workflow.digest(p)}
            for p in (file, self.root / 'resources.json')}}
        workflow.write_json(self.root / 'job.json', job)
        return file

    def test_resume_rejects_changed_artifact_before_network(self):
        file = self.job()
        workflow.verify_job(self.root)
        file.write_bytes(b'tampered')
        with patch.object(workflow, 'load_publisher') as load:
            with self.assertRaisesRegex(RuntimeError, '已改变'):
                workflow.publish_job(self.root, {})
            load.assert_not_called()

    def test_resume_rejects_changed_spec(self):
        self.job()
        (self.root / 'resources.json').write_text('{}')
        with self.assertRaisesRegex(RuntimeError, '已改变'):
            workflow.verify_job(self.root)

    def test_invalid_promotion_does_not_upload_resources(self):
        self.job()
        with patch.object(workflow, 'load_publisher') as load:
            with self.assertRaisesRegex(RuntimeError, '没有软件'):
                workflow.publish_job(self.root, {}, promote=True)
            load.assert_not_called()

    def test_legacy_software_job_reuses_recorded_source_without_rebuild(self):
        self.job()
        source = 'a' * 40
        spec = workflow.read_json(self.root / 'resources.json')
        spec['kind'] = 'software'
        spec['assets'][0]['id'] = 'hub'
        workflow.write_json(self.root / 'resources.json', spec)
        job = workflow.read_json(self.root / 'job.json')
        job['sources'] = {'hub_commit': source}
        job['files']['resources.json'] = {'size': (self.root / 'resources.json').stat().st_size,
                                           'sha256': workflow.digest(self.root / 'resources.json')}
        workflow.write_json(self.root / 'job.json', job)
        original_job = (self.root / 'job.json').read_bytes()
        seen = []
        class Publisher:
            @staticmethod
            def publish(spec, revision, output, config):
                seen.append(spec['assets'][0]['source_commit'])
                workflow.write_json(output / 'publish-report.json', {'published': ['github', 'gitee']})
        with patch.object(workflow, 'load_publisher', return_value=Publisher), \
                patch.object(workflow, 'prepare') as build, \
                patch.object(workflow, 'verify_source_snapshot') as gate, \
                patch.object(workflow, 'run') as command:
            self.assertEqual(workflow.publish_job(self.root, {'publisher_config': 'unused'}), 0)
            build.assert_not_called()
            gate.assert_not_called()
            command.assert_not_called()
        self.assertEqual(seen, [source])
        self.assertNotIn('source_commit', workflow.read_json(self.root / 'resources.json')['assets'][0])
        self.assertEqual((self.root / 'job.json').read_bytes(), original_job)
        self.assertNotIn('verification', workflow.read_json(self.root / 'job.json'))

    def test_software_job_with_conflicting_source_stops_before_publisher(self):
        self.job()
        spec = workflow.read_json(self.root / 'resources.json')
        spec['kind'] = 'software'
        spec['assets'][0].update(id='hub', source_commit='b' * 40)
        workflow.write_json(self.root / 'resources.json', spec)
        job = workflow.read_json(self.root / 'job.json')
        job['sources'] = {'hub_commit': 'a' * 40}
        job['files']['resources.json'] = {'size': (self.root / 'resources.json').stat().st_size,
                                           'sha256': workflow.digest(self.root / 'resources.json')}
        workflow.write_json(self.root / 'job.json', job)
        with patch.object(workflow, 'load_publisher') as load:
            with self.assertRaisesRegex(RuntimeError, '来源提交'):
                workflow.publish_job(self.root, {})
            load.assert_not_called()

    def test_partial_mirror_exit_code_and_resume_without_rebuild(self):
        self.job()
        revisions = []
        class Publisher:
            @staticmethod
            def publish(spec, revision, output, config):
                revisions.append(revision)
                workflow.write_json(output / 'publish-report.json', {'published': ['github']})
        with patch.object(workflow, 'load_publisher', return_value=Publisher), patch.object(workflow, 'prepare') as build:
            self.assertEqual(workflow.publish_job(self.root, {'publisher_config': 'unused'}), 2)
            self.assertEqual(workflow.publish_job(self.root, {'publisher_config': 'unused'}), 2)
            build.assert_not_called()
        self.assertGreater(revisions[1], revisions[0])

    def test_promotion_does_not_downgrade_latest(self):
        calls = []
        class Platform:
            def __init__(self, name, repo, token):
                self.name = name
            def call(self, method, endpoint, **kwargs):
                calls.append((method, endpoint))
                return {'tag_name': 'v2.0.0'}
        with patch('release_platforms.Platform', Platform), patch('release_platforms.credentials', return_value=(
                {'github_repo': 'a/b', 'gitee_repo': 'a/b'}, 'fake')):
            with self.assertRaisesRegex(RuntimeError, '拒绝降低'):
                workflow.promote_software({'version': '1.0.0', 'release_tag': 'v1.0.0'}, {'publisher_config': 'unused'})
        self.assertEqual(calls, [('GET', '/releases/latest')])

    def test_promotion_allows_known_version_correction_with_source_checks(self):
        calls = []
        class Platform:
            def __init__(self, name, repo, token):
                self.name = name
            def call(self, method, endpoint, **kwargs):
                calls.append((self.name, method, endpoint))
                if method == 'PATCH':
                    self_test.assertEqual(kwargs['json']['name'], 'D2RHub v0.9.107')
                    self_test.assertEqual(kwargs['json']['body'], 'Prepared release notes')
                return {'tag_name': 'v0.99.106'}
            def release(self, tag, create=False, source_commit=None):
                if self.name == 'github':
                    self_test.assertEqual(source_commit, 'a' * 40)
                return {'id': 7, 'tag_name': tag, 'name': tag, 'body': 'release'}
        self_test = self
        with patch('release_platforms.Platform', Platform), patch('release_platforms.credentials', return_value=(
                {'github_repo': 'a/b', 'gitee_repo': 'a/b'}, 'fake')):
            workflow.promote_software({'version': '0.9.107', 'release_tag': 'v0.9.107',
                                       'source_commit': 'a' * 40, 'release_name': 'D2RHub v0.9.107',
                                       'release_notes': 'Prepared release notes'}, {'publisher_config': 'unused'})
        self.assertEqual(calls, [('github', 'GET', '/releases/latest'),
                                ('gitee', 'PATCH', '/releases/7'), ('github', 'PATCH', '/releases/7')])

    def test_software_build_version_disagreement_stops_before_build(self):
        source = self.root / 'repo'
        source.mkdir()
        workflow.write_json(source / 'package.json', {'version': '1.0.0'})
        workflow.write_json(source / 'package-lock.json', {'version': '1.0.0', 'packages': {'': {'version': '1.0.0'}}})
        workflow.write_json(source / 'src-tauri/tauri.conf.json', {'version': '1.0.1'})
        (source / 'src-tauri/Cargo.toml').write_text('[package]\nversion = "1.0.0"\n')
        with patch.object(workflow, 'ROOT', source), patch.object(workflow, 'source_commit', return_value='a' * 40), patch.object(workflow, 'source_snapshot', return_value=source), patch.object(workflow, 'run') as run:
            with self.assertRaisesRegex(RuntimeError, '版本号'):
                workflow.build_software(self.root / 'output')
            run.assert_not_called()

    def test_release_description_requires_notes_and_preserves_known_limitations(self):
        with self.assertRaisesRegex(RuntimeError, '缺少软件发行说明'):
            workflow.software_release_description(self.root, '1.0.0', 'a' * 40)
        notes = self.root / 'docs/releases/v1.0.0.md'
        notes.parent.mkdir(parents=True)
        notes.write_text('# v1.0.0\n', encoding='utf-8')
        with self.assertRaisesRegex(RuntimeError, '发行说明为空'):
            workflow.software_release_description(self.root, '1.0.0', 'a' * 40)
        notes.write_text('# v1.0.0\n\n- 更新\n\n已知限制：大厅识别。', encoding='utf-8')
        description = workflow.software_release_description(self.root, '1.0.0', 'a' * 40, '2026-11-13')
        self.assertEqual(description['release_name'], 'D2RHub v1.0.0')
        self.assertIn('D2RHub v1.0.0 · 2026-11-13', description['release_notes'])
        self.assertIn('已知限制：大厅识别。', description['release_notes'])
        self.assertIn('来源提交：`' + 'a' * 40 + '`', description['release_notes'])

    def software_repo(self):
        repo = self.root / 'hub-repo'
        workflow.write_json(repo / 'package.json', {'version': '1.0.0'})
        workflow.write_json(repo / 'package-lock.json', {'version': '1.0.0', 'packages': {'': {'version': '1.0.0'}}})
        workflow.write_json(repo / 'src-tauri/tauri.conf.json', {'version': '1.0.0'})
        (repo / 'src-tauri/Cargo.toml').write_text('[package]\nversion = "1.0.0"\n')
        (repo / 'src-tauri/Cargo.lock').write_text('# fixture lockfile\n')
        (repo / 'docs/releases').mkdir(parents=True)
        (repo / 'docs/releases/v1.0.0.md').write_text('# v1.0.0\n\n- Release changes\n', encoding='utf-8')
        processor = self.root / 'd2r-audio-mod'
        processor.mkdir(exist_ok=True)
        (processor / 'LICENSE').write_text('MIT')
        (processor / 'crates/stormlib-sys/LICENSES').mkdir(parents=True, exist_ok=True)
        (processor / 'crates/stormlib-sys/THIRD_PARTY_NOTICES.md').write_text('StormLib')
        (processor / 'Cargo.toml').write_text('[package]\nversion = "1.4.0-beta.17"\n')
        return repo

    def mocked_snapshot(self, repo, destination, product, commit):
        self.assertEqual(commit, 'a' * 40)
        source = destination / f'source-{product}'
        shutil.copytree(repo, source)
        (destination / f'{product}-source.zip').write_bytes(b'archived tracked source')
        return source

    def mocked_build_command(self, args, cwd=workflow.ROOT, env=None, capture=False):
        command = list(map(str, args))
        if command[:3] == ['npm', 'run', 'build:nsis']:
            self.assertEqual(command[3:], ['--', '--', '--locked'])
            executable = Path(env['CARGO_TARGET_DIR']) / 'release/bundle/nsis/hub-setup.exe'
        elif command[:2] == ['cargo', 'build']:
            self.assertIn('--locked', command)
            executable = Path(env['CARGO_TARGET_DIR']) / 'release/d2r-audio-mod.exe'
        elif command[-1] == '--version':
            return 'd2r-audio-mod 1.4.0-beta.17 (protocol v7)'
        else:
            return None
        executable.parent.mkdir(parents=True, exist_ok=True)
        executable.write_bytes(b'verified build bytes')
        return None

    def prepare_mock_software(self):
        repo = self.software_repo()
        cfg = {'output_root': str(self.root / 'jobs')}
        with patch.object(workflow, 'ROOT', repo), \
                patch.object(workflow, 'source_commit', return_value='a' * 40), \
                patch.object(workflow, 'source_snapshot', side_effect=self.mocked_snapshot), \
                patch.object(workflow, 'run', side_effect=self.mocked_build_command) as command, \
                patch('release_platforms.windows_file_version', return_value='1.0.0'):
            folder = workflow.prepare('software', cfg)
        return folder, command.call_args_list

    def test_software_snapshot_passes_every_gate_before_packaging_and_records_its_sha(self):
        folder, commands = self.prepare_mock_software()
        job = workflow.verify_job(folder)
        self.assertEqual(set(job['verification']), {'hub', 'processor'})
        bundled = folder / 'source-hub/src-tauri/processor/d2r-audio-mod.exe'
        self.assertEqual(bundled.read_bytes(), b'verified build bytes')
        self.assertEqual(workflow.read_json(folder / 'software.json')['assets'][0]['id'], 'hub')
        commands = [call for call in commands if 'source-hub' in str(call.args[1])]
        self.assertEqual(job['schema'], 2)
        report = workflow.read_json(folder / job['verification']['hub'])
        self.assertEqual(report['source_commit'], job['sources']['hub_commit'])
        self.assertEqual(report['status'], 'passed')
        self.assertEqual(report['source_archive']['sha256'], workflow.digest(folder / 'hub-source.zip'))
        expected = workflow.quality_commands('hub')
        self.assertEqual([list(call.args[0]) for call in commands[:-1]], [list(cmd) for _, cmd, _ in expected])
        self.assertEqual(list(commands[-1].args[0])[:3], ['npm', 'run', 'build:nsis'])
        for step, call, (_, command, relative) in zip(report['commands'], commands, expected):
            self.assertEqual(step['source_commit'], 'a' * 40)
            self.assertEqual(step['command'], list(command))
            self.assertEqual(step['status'], 'passed')
            self.assertEqual(step['exit_code'], 0)
            self.assertEqual(Path(call.args[1]), folder / step['working_directory'])
            self.assertEqual(call.kwargs['env']['CARGO_TARGET_DIR'], str(folder / 'build-hub'))
        self.assertEqual(Path(commands[-1].args[1]), folder / 'source-hub')

    def test_each_failed_software_gate_stops_bundle_and_leaves_no_publishable_job(self):
        repo = self.software_repo()
        for name, failed_command, _ in workflow.quality_commands('hub'):
            with self.subTest(gate=name):
                cfg = {'output_root': str(self.root / name)}
                def fail(args, cwd=repo, env=None, capture=False):
                    if list(args) == list(failed_command) and "source-hub" in str(cwd):
                        raise subprocess.CalledProcessError(17, args)
                    return self.mocked_build_command(args, cwd, env, capture)
                with patch.object(workflow, 'ROOT', repo), \
                        patch.object(workflow, 'source_commit', return_value='a' * 40), \
                        patch.object(workflow, 'source_snapshot', side_effect=self.mocked_snapshot), \
                        patch.object(workflow, 'run', side_effect=fail) as command, \
                        patch.object(workflow, 'publish_job') as publish:
                    with self.assertRaisesRegex(RuntimeError, name):
                        workflow.prepare('software', cfg)
                    publish.assert_not_called()
                self.assertFalse(any(list(call.args[0])[:3] == ['npm', 'run', 'build:nsis'] for call in command.call_args_list))
                folder = next(Path(cfg['output_root']).iterdir())
                self.assertFalse((folder / 'job.json').exists())
                self.assertFalse((folder / 'software.json').exists())
                report = workflow.read_json(folder / 'verification-hub.json')
                self.assertEqual(report['status'], 'failed')
                self.assertEqual(report['commands'][-1]['id'], name)
                self.assertEqual(report['commands'][-1]['exit_code'], 17)

    def test_failed_gate_in_publish_mode_never_calls_publisher(self):
        repo = self.software_repo()
        cfg = {'output_root': str(self.root / 'jobs')}
        with patch.object(workflow, 'ROOT', repo), \
                patch.object(workflow, 'configuration', return_value=cfg), \
                patch.object(workflow, 'source_commit', return_value='a' * 40), \
                patch.object(workflow, 'source_snapshot', side_effect=self.mocked_snapshot), \
                patch.object(workflow, 'run', side_effect=RuntimeError('check failed')), \
                patch.object(workflow, 'publish_job') as publish, \
                patch.object(sys, 'argv', ['release-workflow.py', '--target', 'software', '--publish']):
            with self.assertRaisesRegex(RuntimeError, '质量检查'):
                workflow.main()
            publish.assert_not_called()

    def test_processor_locked_checks_precede_release_build_in_the_same_snapshot(self):
        repo = self.root / 'processor-repo'
        repo.mkdir()
        (repo / 'Cargo.toml').write_text('[package]\nname = "d2r-audio-mod"\nversion = "1.4.0-beta.17"\n')
        (repo / 'Cargo.lock').write_text('# fixture lockfile\n')
        destination = self.root / 'processor-job'
        destination.mkdir()
        with patch.object(workflow, 'source_commit', return_value='a' * 40), \
                patch.object(workflow, 'source_snapshot', side_effect=self.mocked_snapshot), \
                patch.object(workflow, 'run', side_effect=self.mocked_build_command) as command:
            asset, commit = workflow.build_processor(repo, destination)
        self.assertEqual(commit, 'a' * 40)
        self.assertEqual(asset['version'], '1.4.0-beta.17')
        calls = command.call_args_list
        self.assertEqual([list(call.args[0]) for call in calls[:3]], [list(cmd) for _, cmd, _ in workflow.quality_commands('processor')])
        self.assertEqual(list(calls[3].args[0])[:3], ['cargo', 'build', '--locked'])
        self.assertTrue(all(Path(call.args[1]) == destination / 'source-processor' for call in calls))
        self.assertEqual(workflow.read_json(destination / 'verification-processor.json')['status'], 'passed')

    def test_processor_gate_failure_never_builds_release(self):
        repo = self.root / 'processor-repo'
        repo.mkdir()
        (repo / 'Cargo.toml').write_text('[package]\nversion = "1.4.0-beta.17"\n')
        destination = self.root / 'processor-job'
        destination.mkdir()
        def fail_tests(args, cwd, env=None, capture=False):
            if list(args)[:2] == ['cargo', 'test']:
                raise subprocess.CalledProcessError(1, args)
        with patch.object(workflow, 'source_commit', return_value='a' * 40), \
                patch.object(workflow, 'source_snapshot', side_effect=self.mocked_snapshot), \
                patch.object(workflow, 'run', side_effect=fail_tests) as command:
            with self.assertRaisesRegex(RuntimeError, 'rust-tests'):
                workflow.build_processor(repo, destination)
        self.assertFalse(any(list(call.args[0])[:2] == ['cargo', 'build'] for call in command.call_args_list))
        self.assertFalse(list(destination.glob('*.exe')))

    def test_verified_resume_uses_original_bytes_without_source_or_gate_execution(self):
        folder, _ = self.prepare_mock_software()
        original = {path.name: path.read_bytes() for path in folder.iterdir() if path.is_file()}
        shutil.rmtree(folder / 'source-hub')
        shutil.rmtree(folder / 'build-hub')
        class Publisher:
            @staticmethod
            def publish(spec, revision, output, config):
                workflow.write_json(output / 'publish-report.json', {'published': ['github', 'gitee']})
        with patch.object(workflow, 'load_publisher', return_value=Publisher), \
                patch.object(workflow, 'verify_source_snapshot') as gates, \
                patch.object(workflow, 'source_commit') as source, \
                patch.object(workflow, 'run') as command, \
                patch.object(workflow, 'prepare') as prepare:
            self.assertEqual(workflow.publish_job(folder, {'publisher_config': 'unused'}), 0)
            gates.assert_not_called()
            source.assert_not_called()
            command.assert_not_called()
            prepare.assert_not_called()
        self.assertEqual(original, {name: (folder / name).read_bytes() for name in original})

    def test_schema_two_requires_matching_complete_quality_evidence_before_network(self):
        folder, _ = self.prepare_mock_software()
        report_path = folder / 'verification-hub.json'
        original = workflow.read_json(report_path)
        for mutation in ('source', 'failed', 'missing-command'):
            with self.subTest(mutation=mutation):
                report = json.loads(json.dumps(original))
                if mutation == 'source':
                    report['source_commit'] = 'b' * 40
                elif mutation == 'failed':
                    report['commands'][0]['status'] = 'failed'
                else:
                    report['commands'].pop()
                workflow.write_json(report_path, report)
                job = workflow.read_json(folder / 'job.json')
                job['files'][report_path.name] = {'size': report_path.stat().st_size, 'sha256': workflow.digest(report_path)}
                workflow.write_json(folder / 'job.json', job)
                with patch.object(workflow, 'load_publisher') as load:
                    with self.assertRaisesRegex(RuntimeError, '质量检查'):
                        workflow.publish_job(folder, {})
                    load.assert_not_called()

    def test_processor_version_includes_protocol_suffix(self):
        self.assertTrue(workflow.processor_version_matches('d2r-audio-mod 1.4.0-beta.17 (protocol v7)', '1.4.0-beta.17'))
        self.assertTrue(workflow.processor_version_matches('d2r-audio-mod 1.4.0-beta.17', '1.4.0-beta.17'))
        self.assertFalse(workflow.processor_version_matches('d2r-audio-mod 1.4.0-beta.16 (protocol v7)', '1.4.0-beta.17'))
        self.assertFalse(workflow.processor_version_matches('other-tool 1.4.0-beta.17', '1.4.0-beta.17'))

    def test_build_snapshot_excludes_stale_ignored_permissions(self):
        repo = self.root / 'repo'
        repo.mkdir()
        (repo / '.gitignore').write_text('permissions/\n')
        (repo / 'source.txt').write_text('committed source')
        workflow.run(['git', 'init', '--quiet'], repo)
        workflow.run(['git', 'add', '.'], repo)
        workflow.run(['git', '-c', 'user.name=Workflow test', '-c', 'user.email=test@example.invalid',
                      'commit', '--quiet', '-m', 'fixture'], repo)
        (repo / 'permissions').mkdir()
        (repo / 'permissions/removed-command.toml').write_text('stale generated command')
        commit = workflow.source_commit(repo)
        snapshot = workflow.source_snapshot(repo, self.root, 'hub', commit)
        self.assertEqual((snapshot / 'source.txt').read_text(), 'committed source')
        self.assertFalse((snapshot / 'permissions').exists())
        self.assertTrue((repo / 'permissions/removed-command.toml').exists())


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    unittest.main()
