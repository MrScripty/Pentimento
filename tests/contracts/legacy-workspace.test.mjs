import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const root = fileURLToPath(new URL('../../', import.meta.url));
const metadata = (manifest) => JSON.parse(execFileSync('cargo', [
    'metadata', '--offline', '--no-deps', '--format-version', '1',
    '--manifest-path', path.join(root, manifest),
], { cwd: root, encoding: 'utf8' }));
const memberNames = (data) => data.packages
    .filter((pkg) => data.workspace_members.includes(pkg.id))
    .map((pkg) => pkg.name).sort();

// Cargo, not a home-grown manifest parser, decides membership and target paths.
test('Cargo resolves supported and retained frontends in distinct workspaces', () => {
    const active = metadata('Cargo.toml');
    const legacy = metadata('legacy/Cargo.toml');
    assert.equal(active.workspace_root, root.replace(/\/$/, ''));
    assert.equal(legacy.workspace_root, path.join(root, 'legacy'));
    for (const name of ['pentimento', 'pentimento-webview', 'pentimento-dioxus-ui', 'pentimento-egui-ui', 'pentimento-wasm']) {
        assert.ok(memberNames(active).includes(name), `${name} must remain active`);
    }
    for (const name of ['pentimento', 'pentimento-webview']) {
        const pkg = active.packages.find((entry) => entry.name === name);
        assert.ok(!Object.hasOwn(pkg.features, 'webkit'), `${name} must not advertise WebKit`);
        assert.ok(!pkg.targets.some((entry) => entry.name === 'capture_test'));
    }
    const expected = [
        'pentimento-frontend-overlay', 'pentimento-frontend-webkit',
        'pentimento-legacy', 'pentimento-tauri', 'pentimento-webview-legacy',
    ].sort();
    assert.deepEqual(memberNames(legacy), expected);
    for (const name of expected) assert.ok(!memberNames(active).includes(name), name);

    // Even optional and target-specific retained dependency edges must be absent.
    for (const pkg of active.packages) {
        for (const dep of pkg.dependencies) {
            assert.ok(!/^(?:gtk|gdk|gdkx11|webkit2gtk|cairo-rs|gio|wry|tauri(?:-|$))/.test(dep.name),
                `${pkg.name} leaks retained dependency ${dep.name}`);
        }
    }
});

test('Cargo preserves original legacy binary, example and library source consumers', () => {
    const legacy = metadata('legacy/Cargo.toml');
    const target = (pkg, name) => legacy.packages.find((entry) => entry.name === pkg)
        ?.targets.find((entry) => entry.name === name);
    for (const [pkg, name, source] of [
        ['pentimento-legacy', 'pentimento', 'crates/app/src/main.rs'],
        ['pentimento-webview-legacy', 'pentimento_webview', 'crates/webview/src/lib.rs'],
        ['pentimento-webview-legacy', 'capture_test', 'crates/webview/examples/capture_test.rs'],
        ['pentimento-tauri', 'pentimento-tauri', 'src-tauri/src/main.rs'],
        ['pentimento-tauri', 'pentimento_tauri_lib', 'src-tauri/src/lib.rs'],
    ]) {
        const entry = target(pkg, name);
        assert.ok(entry, `${pkg}/${name} target must exist`);
        assert.equal(fs.realpathSync(entry.src_path), path.join(root, source));
    }
    const app = legacy.packages.find((pkg) => pkg.name === 'pentimento-legacy');
    const dependency = app.dependencies.find((dep) => dep.rename === 'pentimento-webview');
    assert.equal(dependency.name, 'pentimento-webview-legacy');
    assert.ok(app.features.webkit.includes('pentimento-webview/webkit'));
    assert.ok(app.features.webkit.includes('dep:painting'), 'retained IPC dispatcher needs painting');
});

test('retained standalone manifest entrypoints select the same legacy workspace', () => {
    for (const manifest of [
        'src-tauri/Cargo.toml', 'crates/frontend-webkit/Cargo.toml',
        'crates/frontend-overlay/Cargo.toml', 'legacy/app/Cargo.toml',
        'legacy/webview/Cargo.toml',
    ]) {
        assert.equal(metadata(manifest).workspace_root, path.join(root, 'legacy'), manifest);
    }
});
