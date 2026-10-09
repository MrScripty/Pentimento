"""Provision only the pinned official Electron runtime on an ephemeral Linux CI runner.

Verification runs without privileges. Root tools receive verified bytes on stdin,
never a writable checkout path. The helper gains setuid only after the complete
protected copy matches the official archive. No renderer security flags change.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import tempfile
import urllib.parse
import urllib.request
import zipfile

VERSION = '44.5.1'
ARCHIVE_NAME = f'electron-v{VERSION}-linux-x64.zip'
RELEASE_URL = f'https://github.com/electron/electron/releases/download/v{VERSION}'
# Official SHASUMS256.txt and the locked npm package's checksums.json agree.
ARCHIVE_SHA256 = '5bcd217611d6843ececd6c9e9c1fcd1da3ab066c43d8b1a9e4b44689a1fba6f5'
DESTINATION = Path(f'/opt/pentimento-ci/electron-{VERSION}')
MAX_ARCHIVE = 300 * 1024 * 1024
MAX_UNPACKED = 1024 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest_file(path):
    # O_NOFOLLOW rejects a last-component symlink, including a replacement race.
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW), 'rb') as stream:
        require(stat.S_ISREG(os.fstat(stream.fileno()).st_mode), f'Not a regular file: {path}')
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def release_checksum(text):
    matches = re.findall(r'^([a-f0-9]{64}) [ *]' + re.escape(ARCHIVE_NAME) + r'$', text, re.M)
    require(len(matches) == 1, 'Official checksum entry is absent or ambiguous')
    require(matches[0] == ARCHIVE_SHA256, 'Official checksum differs from reviewed version pin')
    return matches[0]


def download(url, destination, limit):
    with urllib.request.urlopen(url, timeout=45) as response:
        resolved = urllib.parse.urlsplit(response.url)
        require(resolved.scheme == 'https' and resolved.hostname in {
            'github.com', 'release-assets.githubusercontent.com', 'objects.githubusercontent.com'
        }, 'Release download redirected outside official HTTPS asset hosts')
        total = 0
        with destination.open('xb') as output:
            while chunk := response.read(1024 * 1024):
                total += len(chunk)
                require(total <= limit, 'Release download exceeds size bound')
                output.write(chunk)


def archive_manifest(archive, expected_digest=ARCHIVE_SHA256):
    with os.fdopen(os.open(archive, os.O_RDONLY | os.O_NOFOLLOW), 'rb') as stream:
        require(stat.S_ISREG(os.fstat(stream.fileno()).st_mode), 'Archive must be a regular file')
        data = stream.read(MAX_ARCHIVE + 1)
    require(len(data) <= MAX_ARCHIVE, 'Electron archive exceeds compressed size bound')
    require(hashlib.sha256(data).hexdigest() == expected_digest, 'Electron archive checksum mismatch')
    files, directories, seen = {}, set(), set()
    # Parse and copy only this immutable snapshot, after hashing these exact bytes.
    with zipfile.ZipFile(io.BytesIO(data)) as source:
        require(sum(item.file_size for item in source.infolist()) <= MAX_UNPACKED,
                'Electron archive exceeds unpacked size bound')
        for item in source.infolist():
            name = item.filename.rstrip('/') if item.is_dir() else item.filename
            require(name and '\\' not in name and not name.startswith('/') and
                    all(part not in {'', '.', '..'} for part in name.split('/')),
                    'Unsafe archive path')
            require(name not in seen, 'Duplicate archive path')
            seen.add(name)
            mode = item.external_attr >> 16
            kind = stat.S_IFMT(mode)
            require(kind in {0, stat.S_IFDIR if item.is_dir() else stat.S_IFREG},
                    'Archive contains a symlink or special file')
            require(not item.flag_bits & 1, 'Encrypted archive entry')
            parents = PurePosixPath(name).parents
            directories.update(str(parent) for parent in parents if str(parent) != '.')
            if item.is_dir():
                directories.add(name)
            elif name != 'electron.d.ts':  # npm's official installer moves this outside dist.
                with source.open(item) as stream:
                    checksum = hashlib.file_digest(stream, 'sha256').hexdigest()
                files[name] = {'sha256': checksum, 'size': item.file_size,
                               'mode': 0o755 if mode & 0o111 else 0o644}
        require({'electron', 'chrome-sandbox', 'version'} <= files.keys(),
                'Official runtime files are missing')
        require(source.read('version').decode().removeprefix('v').strip() == VERSION,
                'Archive runtime version differs from lock')
        require(files['electron']['mode'] == files['chrome-sandbox']['mode'] == 0o755,
                'Official runtime executables are not executable')
    require(not directories.intersection(files), 'Archive file/directory collision')
    return {'files': files, 'directories': sorted(directories), '_archive_bytes': data}


def validate_metadata(metadata, owner, *, directory=False, expected_mode=None):
    require(metadata.st_uid == owner, 'Unsafe file ownership')
    require(stat.S_ISDIR(metadata.st_mode) if directory else stat.S_ISREG(metadata.st_mode),
            'Symlink or special file refused')
    require(not metadata.st_mode & 0o022, 'Group/other writable file refused')
    if owner == 0:
        require(metadata.st_gid == 0, 'Protected runtime must have root group ownership')
    if not directory:
        require(metadata.st_nlink == 1, 'Hard-linked file refused')
    if expected_mode is not None:
        require(stat.S_IMODE(metadata.st_mode) == expected_mode, 'Unexpected runtime permissions')
    else:
        require(not metadata.st_mode & 0o6000, 'Unexpected privileged permissions')


def protected_parent(path):
    for component in reversed((path, *path.parents)):
        validate_metadata(component.lstat(), 0, directory=True)


def source_parents(path, owner):
    for component in reversed((path, *path.parents)):
        metadata = component.lstat()
        require(stat.S_ISDIR(metadata.st_mode) and metadata.st_uid in {0, owner},
                'Unsafe source ancestor ownership or symlink')
        # A private temporary source below the root-owned sticky /tmp is safe.
        shared_tmp = component == Path('/tmp') and metadata.st_uid == 0 and metadata.st_mode & stat.S_ISVTX
        require(not metadata.st_mode & 0o022 or shared_tmp, 'Writable source ancestor refused')


def verify_tree(root, manifest, owner, *, privileged=False):
    validate_metadata(root.lstat(), owner, directory=True)
    entries = list(root.rglob('*'))
    actual_files, actual_directories, verified = set(), set(), {}
    for item in entries:
        name = item.relative_to(root).as_posix()
        metadata = item.lstat()
        if stat.S_ISDIR(metadata.st_mode):
            validate_metadata(metadata, owner, directory=True)
            actual_directories.add(name)
        else:
            expected = manifest['files'].get(name)
            require(expected is not None, f'Unexpected runtime file: {name}')
            mode = 0o4755 if privileged and name == 'chrome-sandbox' else expected['mode']
            validate_metadata(metadata, owner, expected_mode=mode)
            checksum = digest_file(item)
            require(metadata.st_size == expected['size'] and checksum == expected['sha256'],
                    f'Runtime file content mismatch: {name}')
            actual_files.add(name)
            verified[name] = {'sha256': checksum, 'size': metadata.st_size,
                              'uid': metadata.st_uid, 'gid': metadata.st_gid,
                              'mode': f'{stat.S_IMODE(metadata.st_mode):04o}'}
    require(actual_files == manifest['files'].keys(), 'Runtime file set differs from official archive')
    require(actual_directories == set(manifest['directories']), 'Runtime directory set differs from official archive')
    return verified


def install_runtime(manifest, run=subprocess.run):
    protected_parent(DESTINATION.parent.parent)
    require(not os.path.lexists(DESTINATION), 'Existing runtime destination refused')
    if os.path.lexists(DESTINATION.parent):
        protected_parent(DESTINATION.parent)
    else:
        run(['/usr/bin/sudo', '-n', '/usr/bin/install', '-d', '-o', '0', '-g', '0', '-m', '0755',
             str(DESTINATION.parent)], check=True)
        protected_parent(DESTINATION.parent)
    run(['/usr/bin/sudo', '-n', '/usr/bin/mkdir', '-m', '0755', '--', str(DESTINATION)], check=True)
    for name in sorted(manifest['directories'], key=lambda value: (value.count('/'), value)):
        run(['/usr/bin/sudo', '-n', '/usr/bin/mkdir', '-m', '0755', '--', str(DESTINATION / name)], check=True)
    with zipfile.ZipFile(io.BytesIO(manifest['_archive_bytes'])) as source:
        for name, expected in sorted(manifest['files'].items()):
            data = source.read(name)
            require(hashlib.sha256(data).hexdigest() == expected['sha256'], 'Archive changed during copy')
            # Only verified bytes enter the privileged process; no mutable source path is root-read.
            run(['/usr/bin/sudo', '-n', '/usr/bin/install', '-o', '0', '-g', '0', '-m',
                 f"{expected['mode']:04o}", '--', '/dev/stdin', str(DESTINATION / name)],
                input=data, check=True)
    protected_parent(DESTINATION)
    verify_tree(DESTINATION, manifest, 0)
    run(['/usr/bin/sudo', '-n', '/usr/bin/chmod', '4755', '--', str(DESTINATION / 'chrome-sandbox')], check=True)
    return verify_tree(DESTINATION, manifest, 0, privileged=True)


def check_lock(project):
    lock = json.loads((project / 'src-electron/package-lock.json').read_text())
    package = project / 'src-electron/node_modules/electron'
    require(lock['packages']['node_modules/electron']['version'] == VERSION and
            json.loads((package / 'package.json').read_text())['version'] == VERSION,
            'Electron package does not match reviewed lock/version')
    require(json.loads((package / 'checksums.json').read_text())[ARCHIVE_NAME] == ARCHIVE_SHA256,
            'Locked npm checksum differs from official release pin')
    return package / 'dist'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--verify-only', action='store_true', help='No privileged operations')
    parser.add_argument('--evidence', type=Path, required=True)
    args = parser.parse_args()
    require(os.getuid() != 0 and os.geteuid() != 0, 'Provisioning must start as the ordinary user')
    require(os.uname().sysname == 'Linux' and os.uname().machine == 'x86_64', 'Linux x64 runtime only')
    if not args.verify_only:
        require(os.environ.get('GITHUB_ACTIONS') == 'true' and
                os.environ.get('RUNNER_ENVIRONMENT') == 'github-hosted',
                'Privileged provisioning is restricted to ephemeral GitHub-hosted runners')
        release = Path('/etc/os-release').read_text()
        require('ID=ubuntu\n' in release and 'VERSION_ID="24.04"\n' in release,
                'Reviewed Ubuntu 24.04 environment only')
    project = Path(__file__).absolute().parents[2]
    installed = check_lock(project)
    source_parents(installed, os.getuid())
    with tempfile.TemporaryDirectory(prefix='pentimento-electron-release-') as temporary:
        temporary = Path(temporary)
        sums, archive = temporary / 'SHASUMS256.txt', temporary / ARCHIVE_NAME
        download(f'{RELEASE_URL}/SHASUMS256.txt', sums, 1024 * 1024)
        release_checksum(sums.read_text())
        download(f'{RELEASE_URL}/{ARCHIVE_NAME}', archive, MAX_ARCHIVE)
        manifest = archive_manifest(archive)
        verified = verify_tree(installed, manifest, os.getuid())
        if not args.verify_only:
            verified = install_runtime(manifest)
        args.evidence.parent.mkdir(parents=True, exist_ok=True)
        args.evidence.write_text(json.dumps({
            'version': VERSION, 'release': RELEASE_URL, 'archive_sha256': ARCHIVE_SHA256,
            'installed_tree_verified': True, 'protected_copy_verified': not args.verify_only,
            'runtime': str(DESTINATION / 'electron') if not args.verify_only else str(installed / 'electron'),
            'privileged_helper': not args.verify_only, 'provisioning_uid': os.getuid(),
            'files': verified, 'directories': manifest['directories'],
        }, indent=2) + '\n')


if __name__ == '__main__':
    main()
