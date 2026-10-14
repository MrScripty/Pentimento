import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';

const launcher = readFileSync(new URL('../../launcher.sh', import.meta.url), 'utf8');
const install = launcher.match(/^install_electron_node_modules\(\) \{[\s\S]*?^\}/m)?.[0];
assert.ok(install, 'test must execute the production Electron installer function');

function run(t, status) {
    const root = mkdtempSync(join(tmpdir(), 'pentimento-launcher-'));
    t.after(() => rmSync(root, { recursive: true, force: true }));
    mkdirSync(join(root, 'src-electron'));
    return spawnSync('bash', ['-c', `
set -euo pipefail
PROJECT_ROOT="$1"
${install}
npm() {
    printf '%s\\n' "$*"
    if [[ "$1" == ci ]]; then return ${status}; fi
    return 0
}
# ensure_dependency invokes this in a conditional, where errexit is disabled.
if install_electron_node_modules; then exit 0; else exit "$?"; fi
`, 'test-installer', root], { encoding: 'utf8' });
}

test('Electron launcher stops after a failed locked install in conditional context', t => {
    const result = run(t, 41);
    assert.equal(result.status, 41, result.stderr);
    assert.equal(result.stdout, 'ci\n');
});

test('Electron launcher explicitly bootstraps the runtime after successful locked install', t => {
    const result = run(t, 0);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout, 'ci\nrun install:runtime\n');
});


test('Electron release preflight preserves missing-artifact status without launching', () => {
    const preflight = launcher.match(/^require_release_artifacts\(\) \{[\s\S]*?^\}/m)?.[0];
    assert.ok(preflight);
    const result = spawnSync('bash', ['-c', `
set -euo pipefail
FRONTEND=electron
${preflight}
electron_binary_path() { echo 'missing runtime' >&2; return 1; }
require_release_artifacts
`], { encoding: 'utf8' });
    assert.equal(result.status, 4);
    assert.match(result.stderr, /missing runtime/);
});
