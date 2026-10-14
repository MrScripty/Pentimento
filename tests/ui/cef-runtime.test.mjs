import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { cefRuntimeDirectory, lockedCefBinding } from '../../scripts/cef-runtime.mjs';

const binding = '154.5.0+154.0.34';
function fixture(t, version = '154.0.34') {
    const directory = mkdtempSync(join(tmpdir(), 'pentimento-cef-runtime-'));
    t.after(() => rmSync(directory, { recursive: true, force: true }));
    writeFileSync(join(directory, 'archive.json'), JSON.stringify({
        name: `cef_binary_${version}+g14c5a08+chromium-154.0.8037.98_linux64_minimal.tar.bz2`,
        type: 'minimal',
    }));
    for (const file of ['libcef.so', 'icudtl.dat', 'resources.pak', 'chrome_100_percent.pak', 'chrome_200_percent.pak']) {
        writeFileSync(join(directory, file), 'fixture');
    }
    mkdirSync(join(directory, 'locales'));
    return directory;
}

test('CEF runtime resolves only matching packaged resources beside the executable', t => {
    const directory = fixture(t);
    assert.equal(cefRuntimeDirectory(directory, binding), directory);
});

test('CEF runtime rejects older and newer archives rather than selecting a stale cache', t => {
    for (const version of ['143.0.14', '154.0.35']) {
        assert.throws(() => cefRuntimeDirectory(fixture(t, version), binding), /does not match/);
    }
});

test('CEF runtime rejects missing resources and malformed metadata', t => {
    const directory = fixture(t);
    rmSync(join(directory, 'resources.pak'));
    assert.throws(() => cefRuntimeDirectory(directory, binding), /ENOENT/);
    assert.throws(() => cefRuntimeDirectory(directory, '154.5.0'), /must identify/);
    writeFileSync(join(directory, 'archive.json'), '{}');
    assert.throws(() => cefRuntimeDirectory(directory, binding), /does not match/);
});


test('CEF runtime version comes from exactly one generated lock entry', () => {
    const lock = '[[package]]\nname = "cef-dll-sys"\nversion = "154.5.0+154.0.34"\n';
    assert.equal(lockedCefBinding(lock), binding);
    assert.throws(() => lockedCefBinding(''), /Expected one/);
    assert.throws(() => lockedCefBinding(lock + lock), /Expected one/);
});
