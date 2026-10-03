import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { checkInstallation } = require('../../src-electron/check-install.cjs');

function fixture(t) {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'pentimento-electron-install-'));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    const write = (file, value, mode = 0o644) => {
        const full = path.join(root, file);
        fs.mkdirSync(path.dirname(full), { recursive: true });
        fs.writeFileSync(full, typeof value === 'string' ? value : JSON.stringify(value), { mode });
    };
    write('package.json', { devDependencies: { electron: '^44.5.1' } });
    write('package-lock.json', { packages: { '': { devDependencies: { electron: '^44.5.1' } }, 'node_modules/electron': { version: '44.5.1' } } });
    write('node_modules/electron/package.json', { version: '44.5.1', main: 'index.js' });
    write('node_modules/electron/index.js', 'throw new Error("must not import or download");');
    write('node_modules/.bin/electron', '#!/bin/sh\nexit 0\n', 0o755);
    write('node_modules/electron/dist/electron', '#!/bin/sh\nexit 0\n', 0o755);
    write('node_modules/electron/dist/version', '44.5.1\n');
    write('node_modules/electron/path.txt', 'electron');
    return { root, write };
}

test('Electron readiness validates installed files without importing Electron', (t) => {
    const { root } = fixture(t);
    assert.equal(checkInstallation(root, 'linux'), true);
});
for (const [name, mutate] of [
    ['CLI shim without binary', ({ root }) => fs.unlinkSync(path.join(root, 'node_modules/electron/dist/electron'))],
    ['stale installed Electron 38', ({ write }) => write('node_modules/electron/package.json', { version: '38.8.6' })],
    ['stale binary marker', ({ write }) => write('node_modules/electron/dist/version', '38.8.6')],
    ['missing download marker', ({ root }) => fs.unlinkSync(path.join(root, 'node_modules/electron/path.txt'))],
    ['unexpected binary path', ({ write }) => write('node_modules/electron/path.txt', '../../other')],
    ['unexecutable binary', ({ root }) => fs.chmodSync(path.join(root, 'node_modules/electron/dist/electron'), 0o644)],
    ['manifest/lock mismatch', ({ write }) => write('package.json', { devDependencies: { electron: '^45.0.0' } })],
]) {
    test(`Electron readiness rejects ${name}`, (t) => {
        const files = fixture(t);
        mutate(files);
        assert.equal(checkInstallation(files.root, 'linux'), false);
    });
}

import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

function launcherFixture(t, installerExit = 0, npmExit = 0) {
    const repo = fs.mkdtempSync(path.join(os.tmpdir(), 'pentimento-electron-bootstrap-'));
    t.after(() => fs.rmSync(repo, { recursive: true, force: true }));
    const shell = (file, source) => {
        fs.mkdirSync(path.dirname(file), { recursive: true });
        fs.writeFileSync(file, `#!/usr/bin/env bash\nset -euo pipefail\n${source}`, { mode: 0o755 });
    };
    fs.copyFileSync(path.join(repository, 'launcher.sh'), path.join(repo, 'launcher.sh'));
    fs.mkdirSync(path.join(repo, 'node_modules'));
    fs.mkdirSync(path.join(repo, 'src-electron'));
    for (const file of ['package.json', 'package-lock.json', 'check-install.cjs']) {
        fs.copyFileSync(path.join(repository, 'src-electron', file), path.join(repo, 'src-electron', file));
    }
    const trace = path.join(repo, 'trace');
    shell(path.join(repo, 'bin/cargo'), 'exit 0\n');
    shell(path.join(repo, 'bin/rustup'), 'echo wasm32-unknown-unknown\n');
    shell(path.join(repo, 'bin/wasm-bindgen'), 'exit 0\n');
    shell(path.join(repo, 'bin/npm'), `
echo "npm $*" >> "$TRACE"
mkdir -p node_modules/.bin node_modules/electron
printf '{"version":"44.5.1"}' > node_modules/electron/package.json
printf '#!/bin/sh\\nexit 0\\n' > node_modules/.bin/electron
chmod +x node_modules/.bin/electron
cp "$INSTALLER_FIXTURE" node_modules/.bin/install-electron
exit ${npmExit}
`);
    shell(path.join(repo, 'installer-fixture'), `
echo install-electron >> "$TRACE"
if [[ ${installerExit} != 0 ]]; then exit ${installerExit}; fi
mkdir -p node_modules/electron/dist
printf '#!/bin/sh\\nexit 0\\n' > node_modules/electron/dist/electron
chmod +x node_modules/electron/dist/electron
printf '44.5.1\\n' > node_modules/electron/dist/version
printf electron > node_modules/electron/path.txt
`);
    const run = () => spawnSync('bash', ['launcher.sh', '--install'], {
        cwd: repo, encoding: 'utf8',
        env: { ...process.env, PATH: `${repo}/bin:${process.env.PATH}`, TRACE: trace, INSTALLER_FIXTURE: path.join(repo, 'installer-fixture') },
    });
    return { repo, run, lines: () => fs.existsSync(trace) ? fs.readFileSync(trace, 'utf8').trim().split('\n') : [] };
}

test('launcher install explicitly acquires the binary, is repeatable, and repairs a missing binary', (t) => {
    const { repo, run, lines } = launcherFixture(t);
    assert.equal(run().status, 0);
    assert.deepEqual(lines(), ['npm ci', 'install-electron']);
    assert.equal(run().status, 0);
    assert.deepEqual(lines(), ['npm ci', 'install-electron']);
    fs.unlinkSync(path.join(repo, 'src-electron/node_modules/electron/dist/electron'));
    assert.equal(run().status, 0);
    assert.deepEqual(lines(), ['npm ci', 'install-electron', 'npm ci', 'install-electron']);
});

test('launcher propagates official binary installer failure', (t) => {
    const { repo, run, lines } = launcherFixture(t, 19);
    assert.equal(run().status, 1);
    assert.deepEqual(lines(), ['npm ci', 'install-electron']);
    assert.equal(checkInstallation(path.join(repo, 'src-electron'), 'linux'), false);
});

test('launcher stops when npm ci fails after creating a usable installer', (t) => {
    const { repo, run, lines } = launcherFixture(t, 0, 23);
    assert.equal(run().status, 1);
    assert.deepEqual(lines(), ['npm ci']);
    assert.ok(fs.existsSync(path.join(repo, 'src-electron/node_modules/.bin/install-electron')));
    assert.equal(checkInstallation(path.join(repo, 'src-electron'), 'linux'), false);
});
