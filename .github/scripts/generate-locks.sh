#!/usr/bin/env bash
# Temporary hosted evidence procedure. Not a canonical repository script.
set -euo pipefail
mkdir -p lockfile-evidence/src-electron
{
  git rev-parse HEAD
  node --version
  npm --version
  npm config get registry
} > lockfile-evidence/provenance.txt
[[ $(node --version) == v22.23.3 ]]
[[ $(npm --version) == 10.9.9 ]]
[[ $(npm config get registry) == https://registry.npmjs.org/ ]]
sha256sum package.json src-electron/package.json > lockfile-evidence/manifests.sha256
for root in . src-electron; do
  (
    cd "$root"
    test ! -e package-lock.json
    test ! -d node_modules
    npm install --package-lock-only --ignore-scripts --strict-peer-deps --no-audit --no-fund
  )
done
sha256sum --check lockfile-evidence/manifests.sha256
cp package-lock.json lockfile-evidence/package-lock.json
cp src-electron/package-lock.json lockfile-evidence/src-electron/package-lock.json
sha256sum package-lock.json src-electron/package-lock.json > lockfile-evidence/locks.sha256
for root in . src-electron; do
  destination="$PWD/lockfile-evidence/$root"
  for scope in all production; do
    extra=()
    [[ "$scope" != production ]] || extra+=(--omit=dev)
    status=0
    (cd "$root" && npm audit --package-lock-only --json "${extra[@]}") > "$destination/audit-$scope.json" || status=$?
    printf '%s\n' "$status" > "$destination/audit-$scope.exit"
    # A vulnerability result is evidence requiring review, not a successful audit.
    # Infrastructure errors are recorded separately and block acceptance too.
  done
done
python3 - <<'PY'
import json, pathlib, urllib.parse
out = {}
for root in (pathlib.Path('.'), pathlib.Path('src-electron')):
    manifest = json.loads((root / 'package.json').read_text())
    lock = json.loads((root / 'package-lock.json').read_text())
    assert lock['lockfileVersion'] == 3
    assert lock['name'] == manifest['name']
    assert lock['version'] == manifest['version']
    for key in ('dependencies', 'devDependencies', 'optionalDependencies'):
        assert lock['packages'][''].get(key, {}) == manifest.get(key, {})
    packages = []
    for name, package in lock['packages'].items():
        if not name:
            continue
        assert not package.get('link'), name
        source = package
        if not package.get('resolved'):
            # Bundled children are authenticated by their containing tarball.
            assert package.get('inBundle') is True, name
            parent_path, child_name = name.rsplit('/node_modules/', 1)
            source = lock['packages'][parent_path]
            assert child_name in source.get('bundleDependencies', []), name
            package = {**package, 'verifiedBundleParent': parent_path}
        url = urllib.parse.urlparse(source.get('resolved', ''))
        assert url.scheme == 'https' and url.hostname == 'registry.npmjs.org', (name, url.geturl())
        assert source.get('integrity', '').startswith('sha512-'), name
        packages.append({'path': name, **{key: package[key] for key in ('version', 'resolved', 'integrity', 'dev', 'peer', 'peerDependencies', 'peerDependenciesMeta', 'engines', 'hasInstallScript', 'inBundle', 'verifiedBundleParent') if key in package}})
    out[str(root)] = packages
pathlib.Path('lockfile-evidence/resolved-inventory.json').write_text(json.dumps(out, indent=2) + '\n')
PY
sha256sum --check lockfile-evidence/locks.sha256
