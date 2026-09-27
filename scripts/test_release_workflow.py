import importlib.util
import json
from pathlib import Path
import tempfile
import sys
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

    def test_software_build_version_disagreement_stops_before_build(self):
        source = self.root / 'repo'
        source.mkdir()
        workflow.write_json(source / 'package.json', {'version': '1.0.0'})
        workflow.write_json(source / 'package-lock.json', {'version': '1.0.0', 'packages': {'': {'version': '1.0.0'}}})
        workflow.write_json(source / 'src-tauri/tauri.conf.json', {'version': '1.0.1'})
        (source / 'src-tauri/Cargo.toml').write_text('[package]\nversion = "1.0.0"\n')
        with patch.object(workflow, 'ROOT', source), patch.object(workflow, 'source_commit', return_value='commit'), patch.object(workflow, 'run') as run:
            with self.assertRaisesRegex(RuntimeError, '版本号'):
                workflow.build_software(self.root / 'output')
            run.assert_not_called()

    def test_processor_version_includes_protocol_suffix(self):
        self.assertTrue(workflow.processor_version_matches('d2r-audio-mod 1.4.0-beta.17 (protocol v7)', '1.4.0-beta.17'))
        self.assertTrue(workflow.processor_version_matches('d2r-audio-mod 1.4.0-beta.17', '1.4.0-beta.17'))
        self.assertFalse(workflow.processor_version_matches('d2r-audio-mod 1.4.0-beta.16 (protocol v7)', '1.4.0-beta.17'))
        self.assertFalse(workflow.processor_version_matches('other-tool 1.4.0-beta.17', '1.4.0-beta.17'))


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    unittest.main()
