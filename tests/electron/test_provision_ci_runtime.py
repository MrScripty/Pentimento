"""Offline safety regressions; no sudo, setuid, or /opt mutations are executed."""
import hashlib
import importlib.util
import os
from pathlib import Path
import stat
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location('provision', Path(__file__).with_name('provision_ci_runtime.py'))
provision = importlib.util.module_from_spec(spec)
spec.loader.exec_module(provision)


class ProvisionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.archive = self.root / 'runtime.zip'
        self.files = {'electron': b'ELF-test-electron', 'chrome-sandbox': b'ELF-test-helper',
                      'version': provision.VERSION.encode(), 'locales/en.pak': b'locale'}
        self.write_archive()
        self.manifest = provision.archive_manifest(self.archive, provision.digest_file(self.archive))
        self.source = self.root / 'dist'
        self.source.mkdir()
        for name, data in self.files.items():
            target = self.source / name
            target.parent.mkdir(exist_ok=True)
            target.write_bytes(data)
            target.chmod(self.manifest['files'][name]['mode'])

    def write_archive(self, extra=None):
        with zipfile.ZipFile(self.archive, 'w') as archive:
            for name, data in self.files.items():
                item = zipfile.ZipInfo(name)
                item.external_attr = (stat.S_IFREG | (0o755 if name in {'electron', 'chrome-sandbox'} else 0o644)) << 16
                archive.writestr(item, data)
            if extra:
                archive.writestr(*extra)

    def test_complete_tree_matches_and_requires_every_official_file(self):
        receipt = provision.verify_tree(self.source, self.manifest, os.getuid())
        self.assertEqual(receipt['chrome-sandbox']['mode'], '0755')
        self.assertEqual(receipt['chrome-sandbox']['uid'], os.getuid())
        self.assertEqual(receipt['chrome-sandbox']['sha256'], hashlib.sha256(self.files['chrome-sandbox']).hexdigest())
        (self.source / 'locales/en.pak').unlink()
        with self.assertRaisesRegex(ValueError, 'file set'):
            provision.verify_tree(self.source, self.manifest, os.getuid())

    def test_archive_checksum_and_ambiguous_release_record_are_rejected(self):
        with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
            provision.archive_manifest(self.archive)
        line = f'{provision.ARCHIVE_SHA256} *{provision.ARCHIVE_NAME}\n'
        self.assertEqual(provision.release_checksum(line), provision.ARCHIVE_SHA256)
        for text in ['', line + line, line.replace(provision.ARCHIVE_SHA256, '0' * 64)]:
            with self.subTest(text=text), self.assertRaises(ValueError):
                provision.release_checksum(text)

    def test_traversal_symlinks_special_files_and_collisions_are_rejected(self):
        for name, mode in [('../escape', stat.S_IFREG), ('/absolute', stat.S_IFREG),
                           ('a\\b', stat.S_IFREG), ('link', stat.S_IFLNK),
                           ('device', stat.S_IFCHR), ('electron/child', stat.S_IFREG)]:
            item = zipfile.ZipInfo(name)
            item.external_attr = (mode | 0o644) << 16
            self.write_archive((item, b'bad'))
            with self.subTest(name=name), self.assertRaises(ValueError):
                provision.archive_manifest(self.archive, provision.digest_file(self.archive))

    def test_installed_tampering_extra_files_and_symlinks_are_rejected(self):
        helper = self.source / 'chrome-sandbox'
        helper.write_bytes(b'tampered')
        with self.assertRaisesRegex(ValueError, 'content mismatch'):
            provision.verify_tree(self.source, self.manifest, os.getuid())
        helper.write_bytes(self.files['chrome-sandbox'])
        helper.unlink()
        helper.symlink_to(self.source / 'electron')
        with self.assertRaisesRegex(ValueError, 'Symlink'):
            provision.verify_tree(self.source, self.manifest, os.getuid())
        helper.unlink()
        helper.write_bytes(self.files['chrome-sandbox'])
        helper.chmod(0o755)
        (self.source / 'extra').write_bytes(b'unknown')
        with self.assertRaisesRegex(ValueError, 'Unexpected runtime file'):
            provision.verify_tree(self.source, self.manifest, os.getuid())

    def test_unsafe_ownership_permissions_and_hardlinks_are_rejected(self):
        item = self.source / 'electron'
        with self.assertRaisesRegex(ValueError, 'ownership'):
            provision.validate_metadata(item.stat(), os.getuid() + 1)
        item.chmod(0o777)
        with self.assertRaisesRegex(ValueError, 'writable'):
            provision.verify_tree(self.source, self.manifest, os.getuid())
        item.chmod(0o755)
        os.link(item, self.root / 'hardlink')
        with self.assertRaisesRegex(ValueError, 'Hard-linked'):
            provision.verify_tree(self.source, self.manifest, os.getuid())

    def test_source_ancestor_symlinks_are_rejected(self):
        link = self.root / 'linked-dist'
        link.symlink_to(self.source, target_is_directory=True)
        original = Path.lstat
        def hosted_tmp(path, *args, **kwargs):
            metadata = original(path, *args, **kwargs)
            if path in {Path('/'), Path('/tmp')}:
                values = list(metadata)
                values[4:6] = [0, 0]
                return os.stat_result(values)
            return metadata
        with patch.object(Path, 'lstat', hosted_tmp):
            provision.source_parents(self.source, os.getuid())
            with self.assertRaisesRegex(ValueError, 'ancestor'):
                provision.source_parents(link, os.getuid())

    def test_existing_destination_refuses_privileged_commands(self):
        calls = []
        with patch.object(provision, 'DESTINATION', self.source), patch.object(provision, 'protected_parent'):
            with self.assertRaisesRegex(ValueError, 'Existing runtime'):
                provision.install_runtime(self.manifest, lambda *a, **kw: calls.append(a))
        self.assertEqual(calls, [])

    def test_bad_copy_prevents_helper_privilege_and_root_reads_only_stdin(self):
        calls = []
        target = self.root / 'protected' / 'runtime'
        def record(command, **kwargs):
            calls.append((command, kwargs))
        with patch.object(provision, 'DESTINATION', target), patch.object(provision, 'protected_parent'), \
                patch.object(provision, 'verify_tree', side_effect=ValueError('copy hash mismatch')):
            with self.assertRaisesRegex(ValueError, 'copy hash'):
                provision.install_runtime(self.manifest, record)
        self.assertFalse(any('/usr/bin/chmod' in command for command, _ in calls))
        copies = [(command, kwargs) for command, kwargs in calls if 'input' in kwargs]
        self.assertEqual(len(copies), len(self.files))
        for command, kwargs in copies:
            self.assertEqual(command[0:2], ['/usr/bin/sudo', '-n'])
            self.assertEqual(command[-2], '/dev/stdin')
            self.assertNotIn('4755', command)
            self.assertTrue(kwargs['check'])
            self.assertIn(kwargs['input'], self.files.values())

    def test_privilege_is_enabled_only_after_complete_copy_verification(self):
        events = []
        target = self.root / 'protected' / 'runtime'
        def record(command, **kwargs):
            events.append(('command', command))
        def verify(*args, **kwargs):
            events.append(('verified', kwargs.get('privileged', False)))
        with patch.object(provision, 'DESTINATION', target), patch.object(provision, 'protected_parent'), \
                patch.object(provision, 'verify_tree', side_effect=verify):
            provision.install_runtime(self.manifest, record)
        privilege = next(i for i, event in enumerate(events) if event[0] == 'command' and '/usr/bin/chmod' in event[1])
        self.assertEqual(events[privilege - 1], ('verified', False))
        self.assertEqual(events[privilege + 1], ('verified', True))

    def test_archive_replacement_cannot_change_verified_install_bytes(self):
        self.archive.write_bytes(b'replacement after successful checksum')
        calls = []
        with patch.object(provision, 'DESTINATION', self.root / 'protected/runtime'), \
                patch.object(provision, 'protected_parent'), patch.object(provision, 'verify_tree'):
            provision.install_runtime(self.manifest, lambda command, **kw: calls.append((command, kw)))
        self.assertEqual({kw['input'] for _, kw in calls if 'input' in kw}, set(self.files.values()))

    def test_archive_symlink_is_not_followed(self):
        link = self.root / 'linked-archive'
        link.symlink_to(self.archive)
        with self.assertRaises(OSError):
            provision.archive_manifest(link, provision.digest_file(self.archive))

    def test_main_refuses_local_or_self_hosted_privileged_execution_before_io(self):
        with patch('sys.argv', ['provision', '--evidence', str(self.root / 'receipt.json')]), \
                patch.dict(os.environ, {'GITHUB_ACTIONS': 'true', 'RUNNER_ENVIRONMENT': 'self-hosted'}), \
                patch.object(provision, 'download') as download:
            with self.assertRaisesRegex(ValueError, 'ephemeral GitHub-hosted'):
                provision.main()
            download.assert_not_called()


if __name__ == '__main__':
    unittest.main()
