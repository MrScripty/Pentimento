import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';

const read = (path) => fs.readFileSync(new URL(`../../${path}`, import.meta.url), 'utf8');

const appManifest = read('crates/app/Cargo.toml');
const webviewManifest = read('crates/webview/Cargo.toml');
const legacyAppManifest = read('legacy/app/Cargo.toml');
const legacyWebviewManifest = read('legacy/webview/Cargo.toml');

const featureSectionOf = (manifest) =>
    manifest.match(/^\[features\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m)?.[1];

const featureDecl = (section, name) => {
    assert.ok(section, 'Cargo.toml must declare a features section');
    const decl = section.match(new RegExp(`^${name}\\s*=\\s*\\[([^\\]]*)\\]`, 'm'));
    assert.ok(decl, `${name} feature must exist`);
    return decl[1];
};

// The line(s) immediately above `needle` must carry a cfg mentioning every
// entry in `features`, so the gated item is compiled only for those features.
const assertCfgGated = (source, path, needle, features) => {
    const lines = source.split('\n');
    const idx = lines.findIndex((line) => line.includes(needle));
    assert.ok(idx > 0, `${path} must reference ${needle}`);
    const guard = lines.slice(Math.max(0, idx - 12), idx).join('\n');
    assert.match(guard, /#\[cfg\(/, `${path}: ${needle} must be cfg-gated`);
    for (const feature of features) {
        assert.ok(
            guard.includes(`feature = "${feature}"`),
            `${path}: ${needle} must be gated on feature "${feature}"`,
        );
    }
};

test('app webview integration is optional so egui builds skip it', () => {
    assert.match(
        appManifest,
        /^pentimento-webview\s*=\s*\{[^}]*optional\s*=\s*true/m,
        'pentimento-webview must be optional',
    );
    assert.match(
        appManifest,
        /^pentimento-webview\s*=\s*\{[^}]*default-features\s*=\s*false/m,
        'pentimento-webview must not bring default (webkit) features implicitly',
    );
    assert.match(
        appManifest,
        /^rust-embed\s*=\s*\{[^}]*optional\s*=\s*true/m,
        'rust-embed (embedded web UI) must be optional',
    );
    assert.doesNotMatch(appManifest, /^gtk\s*=/m, 'GTK belongs only to the legacy workspace');
});

test('legacy app webkit feature owns the retained capture/overlay backend', () => {
    assert.doesNotMatch(featureSectionOf(appManifest), /^webkit\s*=/m);
    const section = featureSectionOf(legacyAppManifest);
    const decl = featureDecl(section, 'webkit');
    assert.match(decl, /"dep:pentimento-webview"/);
    assert.match(decl, /"pentimento-webview\/webkit"/);
    assert.match(decl, /"dep:rust-embed"/);
});

test('app egui feature pulls no web, dioxus, or cef stack', () => {
    const decl = featureDecl(featureSectionOf(appManifest), 'egui');
    assert.doesNotMatch(decl, /webview/i);
    assert.doesNotMatch(decl, /webkit/i);
    assert.doesNotMatch(decl, /dioxus/i);
    assert.doesNotMatch(decl, /cef/i);
    assert.doesNotMatch(decl, /gtk/i);
    assert.doesNotMatch(decl, /rust-embed/i);
    assert.doesNotMatch(decl, /wry/i);
});

test('app dioxus feature uses no webview backend', () => {
    const decl = featureDecl(featureSectionOf(appManifest), 'dioxus');
    assert.doesNotMatch(decl, /webview/i);
    assert.doesNotMatch(decl, /pollster/i);
});

test('app cef feature skips the webkit stack', () => {
    const decl = featureDecl(featureSectionOf(appManifest), 'cef');
    assert.doesNotMatch(decl, /pentimento-webview\/webkit/);
    assert.match(decl, /"dep:pentimento-webview"/);
});

test('app drops directly unused dependencies', () => {
    for (const name of ['tokio', 'serde', 'tracing', 'image', 'bytemuck']) {
        assert.doesNotMatch(
            appManifest,
            new RegExp(`^${name}\\s*=`, 'm'),
            `${name} is unused by the app and must be removed`,
        );
    }
    assert.doesNotMatch(appManifest, /pentimento-diffusion/);
    assert.doesNotMatch(appManifest, /pollster/);
    assert.doesNotMatch(appManifest, /local-diffusion/);
});

test('app keeps test-only serialization support out of the main graph', () => {
    const devSection = appManifest.match(/^\[dev-dependencies\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m)?.[1];
    assert.ok(devSection, 'app must declare dev-dependencies for test-only crates');
    assert.match(devSection, /^serde_json\s*=/m);
    const mainSection = appManifest.split('[dev-dependencies]')[0];
    assert.doesNotMatch(mainSection, /^serde_json\s*=/m);
});

test('legacy webview webkit stack remains optional behind the webkit feature', () => {
    for (const name of ['wry', 'gtk', 'gdk', 'gdkx11', 'webkit2gtk', 'cairo-rs', 'gio']) {
        const escaped = name.replace(/-/g, '\\-');
        assert.match(
            legacyWebviewManifest,
            new RegExp(`^${escaped}\\s*=\\s*\\{[^}]*optional\\s*=\\s*true`, 'm'),
            `${name} must be optional`,
        );
    }
    const decl = featureDecl(featureSectionOf(legacyWebviewManifest), 'webkit');
    for (const name of ['wry', 'gtk', 'gdk', 'gdkx11', 'webkit2gtk', 'cairo-rs', 'gio']) {
        assert.ok(decl.includes(`"dep:${name}"`), `webkit must enable dep:${name}`);
    }
});

test('only legacy webview defaults to webkit for capture/overlay callers', () => {
    assert.equal(featureDecl(featureSectionOf(webviewManifest), 'default').trim(), '');
    const decl = featureDecl(featureSectionOf(legacyWebviewManifest), 'default');
    assert.match(decl, /"webkit"/);
});

test('webview drops directly unused dependencies', () => {
    assert.doesNotMatch(webviewManifest, /^pentimento-config\s*=/m);
    assert.doesNotMatch(webviewManifest, /^serde\s*=/m);
    assert.doesNotMatch(webviewManifest, /^glib\s*=/m);
    assert.doesNotMatch(webviewManifest, /dev-dependencies/);
    assert.doesNotMatch(webviewManifest, /windows\s*=\s*\{[^}]*0\.58/);
});

test('legacy capture example requires the webkit feature and keeps its source entrypoint', () => {
    assert.match(legacyWebviewManifest, /\[\[example\]\]\s*\nname\s*=\s*"capture_test"/);
    assert.match(legacyWebviewManifest, /required-features\s*=\s*\["webkit"\]/);
    assert.match(legacyWebviewManifest, /path\s*=\s*"\.\.\/\.\.\/crates\/webview\/examples\/capture_test.rs"/);
    assert.match(webviewManifest, /^autoexamples\s*=\s*false/m);
});

test('native webview backends compile only with webkit', () => {
    const lib = read('crates/webview/src/lib.rs');
    assertCfgGated(lib, 'crates/webview/src/lib.rs', 'mod platform_linux;', ['webkit']);
    assertCfgGated(lib, 'crates/webview/src/lib.rs', 'mod platform_linux_overlay;', ['webkit']);
    assertCfgGated(lib, 'crates/webview/src/lib.rs', 'pub struct OffscreenWebview', ['webkit']);
    assertCfgGated(lib, 'crates/webview/src/lib.rs', 'pub struct OverlayWebview', ['webkit']);
});

test('app web UI modules compile only for web-backed frontends', () => {
    const main = read('crates/app/src/main.rs');
    assertCfgGated(main, 'crates/app/src/main.rs', 'mod embedded_ui;', ['webkit', 'cef']);
    assertCfgGated(main, 'crates/app/src/main.rs', 'gtk::init()', ['webkit']);

    const render = read('crates/app/src/render/mod.rs');
    assertCfgGated(render, 'crates/app/src/render/mod.rs', 'OffscreenWebview::new', ['webkit']);
    assertCfgGated(render, 'crates/app/src/render/mod.rs', 'OverlayWebview::new', ['webkit']);
});


test('active manifests have no retained WebKit or Tauri dependency edges', () => {
    for (const manifest of [appManifest, webviewManifest]) {
        for (const name of ['gtk', 'gdk', 'gdkx11', 'webkit2gtk', 'cairo-rs', 'gio', 'wry', 'tauri']) {
            assert.doesNotMatch(manifest, new RegExp(`^${name}\\s*=`, 'm'));
        }
        assert.doesNotMatch(featureSectionOf(manifest), /^webkit\s*=/m);
    }
});

test('retained manifests explicitly belong to the isolated legacy workspace', () => {
    const root = read('Cargo.toml');
    const members = root.match(/^members\s*=\s*\[([^\]]*)\]/m)?.[1];
    assert.ok(members);
    assert.doesNotMatch(members, /src-tauri|frontend-webkit|frontend-overlay|legacy/);
    const legacy = read('legacy/Cargo.toml');
    for (const path of ['../src-tauri', '../crates/frontend-webkit', '../crates/frontend-overlay', 'app', 'webview']) {
        assert.ok(legacy.includes(`"${path}"`));
    }
    for (const path of ['crates/frontend-webkit', 'crates/frontend-overlay']) {
        assert.match(read(`${path}/Cargo.toml`), /^workspace\s*=\s*"\.\.\/\.\.\/legacy"/m);
    }
    assert.match(read('src-tauri/Cargo.toml'), /^workspace\s*=\s*"\.\.\/legacy"/m);
    assert.match(legacyAppManifest, /path\s*=\s*"\.\.\/\.\.\/crates\/app\/src\/main.rs"/);
    assert.match(legacyWebviewManifest, /path\s*=\s*"\.\.\/\.\.\/crates\/webview\/src\/lib.rs"/);
});
