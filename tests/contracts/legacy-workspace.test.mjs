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

test('Cargo retains the official patched Tauri host and generator cohort without new host features', () => {
    const legacy = metadata('legacy/Cargo.toml');
    const app = legacy.packages.find((pkg) => pkg.name === 'pentimento-tauri');
    // The tauri-v2.11.6 tagged helpers match the host API. Macros 2.7 generate
    // Error::UnexpectedMenuKind, which this host does not define. Exact Cargo
    // requirements protect that seam even when the independent lock is rebuilt.
    // https://github.com/tauri-apps/tauri/tree/tauri-v2.11.6/crates
    for (const [name, version, kind, defaults] of [
        ['tauri', '2.11.6', null, false],
        ['tauri-runtime', '2.11.3', null, true],
        ['tauri-runtime-wry', '2.11.4', null, false],
        ['tauri-macros', '2.6.3', null, false],
        ['tauri-build', '2.6.3', 'build', true],
        ['tauri-codegen', '2.6.3', 'build', false],
        ['tauri-plugin', '2.6.3', 'build', false],
        ['tauri-utils', '2.9.3', 'build', false],
    ]) {
        const edges = app.dependencies.filter((dep) => dep.name === name);
        assert.equal(edges.length, 1, `${name} must have one constrained dependency`);
        const [dep] = edges;
        assert.equal(dep.req, `=${version}`, `${name} must not float independently`);
        assert.equal(dep.kind, kind, `${name} must retain its dependency role`);
        assert.equal(dep.uses_default_features, defaults, `${name} default feature policy`);
        assert.equal(dep.optional, false, `${name} constraint must always apply`);
        assert.equal(dep.target, null, `${name} constraint must apply on every target`);
        if (name !== 'tauri') assert.deepEqual(dep.features, [], `${name} feature policy`);
    }
    const host = app.dependencies.find((dep) => dep.name === 'tauri');
    assert.deepEqual([...host.features].sort(), [
        'common-controls-v6', 'compression', 'devtools', 'dynamic-acl', 'wry', 'x11',
    ], 'retain historical host defaults explicitly, without dbus or tray-icon');
    assert.deepEqual(app.features, {
        'custom-protocol': ['tauri/custom-protocol'],
        default: ['custom-protocol'],
    }, 'retain the original custom-protocol entrypoint and default');
});
