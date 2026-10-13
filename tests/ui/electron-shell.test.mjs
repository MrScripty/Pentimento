import test from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';

const ts = createRequire(import.meta.url)('typescript');
const { checkRuntime } = createRequire(import.meta.url)('../../src-electron/check-runtime.cjs');
const shellDirectory = fileURLToPath(new URL('../../src-electron/', import.meta.url));
const compiledDirectory = path.join(shellDirectory, 'dist');

function compile(file) {
    return ts.transpileModule(readFileSync(path.join(shellDirectory, file), 'utf8'), {
        compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020, esModuleInterop: true },
    }).outputText;
}

async function launch(env = {}) {
    const windows = [];
    const app = new EventEmitter();
    app.whenReady = () => Promise.resolve();
    app.quit = () => { app.quitCalled = true; };

    class BrowserWindow extends EventEmitter {
        constructor(options) {
            super();
            this.options = options;
            this.webContents = { openDevTools: () => { this.devToolsOpened = true; } };
            windows.push(this);
        }
        loadFile(file) { this.loadedFile = file; return Promise.resolve(); }
        loadURL(url) { this.loadedURL = url; return Promise.resolve(); }
    }

    runInNewContext(compile('main.ts'), {
        exports: {},
        __dirname: compiledDirectory,
        process: { env },
        require: name => {
            if (name === 'electron') return { app, BrowserWindow };
            if (name === 'path' || name === 'node:path') return path;
            throw new Error(`Unexpected shell dependency: ${name}`);
        },
    });
    await Promise.resolve();
    return { app, windows };
}

test('Electron production shell loads the launcher UI output with a sandboxed preload', async () => {
    const { windows } = await launch();
    assert.equal(windows.length, 1);
    const window = windows[0];
    assert.equal(window.loadedFile, fileURLToPath(new URL('../../dist/ui/index.html', import.meta.url)));
    assert.equal(window.loadedURL, undefined);
    assert.equal(window.devToolsOpened, undefined);
    assert.equal(window.options.webPreferences.preload, path.join(compiledDirectory, 'preload.js'));
    assert.equal(window.options.webPreferences.sandbox, true);
    assert.equal(window.options.webPreferences.contextIsolation, true);
    assert.equal(window.options.webPreferences.nodeIntegration, false);
});

test('Electron development shell loads the configured Vite server without weakening isolation', async () => {
    const { windows } = await launch({ VITE_DEV_SERVER_URL: 'http://127.0.0.1:5173' });
    const window = windows[0];
    assert.equal(window.loadedURL, 'http://127.0.0.1:5173');
    assert.equal(window.loadedFile, undefined);
    assert.equal(window.devToolsOpened, true);
    assert.equal(window.options.webPreferences.sandbox, true);
    assert.equal(window.options.webPreferences.contextIsolation, true);
    assert.equal(window.options.webPreferences.nodeIntegration, false);
});

test('Electron activation reuses an open window and recreates one only after it closes', async () => {
    const { app, windows } = await launch();
    app.emit('activate');
    assert.equal(windows.length, 1);
    windows[0].emit('closed');
    app.emit('activate');
    assert.equal(windows.length, 2);
    assert.equal(windows[1].loadedFile, windows[0].loadedFile);
    app.emit('window-all-closed');
    assert.equal(app.quitCalled, true);
});

test('Electron preload exposes only the runtime marker through contextBridge', () => {
    const exposed = [];
    runInNewContext(compile('preload.ts'), {
        exports: {},
        require: name => {
            assert.equal(name, 'electron');
            return { contextBridge: { exposeInMainWorld: (key, value) => exposed.push([key, value]) } };
        },
    });
    assert.deepEqual(exposed, [['__ELECTRON__', true]]);
});

function runtimeFixture(t) {
    const directory = mkdtempSync(path.join(os.tmpdir(), 'pentimento-electron-runtime-'));
    t.after(() => rmSync(directory, { recursive: true, force: true }));
    const runtimeDirectory = path.join(directory, 'node_modules', 'electron');
    mkdirSync(path.join(runtimeDirectory, 'dist'), { recursive: true });
    const manifest = JSON.parse(readFileSync(path.join(shellDirectory, 'package.json'), 'utf8'));
    const version = manifest.devDependencies.electron;
    writeFileSync(path.join(directory, 'package.json'), JSON.stringify(manifest));
    writeFileSync(path.join(runtimeDirectory, 'package.json'), JSON.stringify({ version }));
    writeFileSync(path.join(runtimeDirectory, 'dist', 'version'), version);
    writeFileSync(path.join(runtimeDirectory, 'path.txt'), 'electron');
    const executable = path.join(runtimeDirectory, 'dist', 'electron');
    // Never executed: this fixture proves read-only preflight, not runtime health.
    writeFileSync(executable, '', { mode: 0o755 });
    writeFileSync(path.join(runtimeDirectory, 'index.js'), 'throw new Error("Preflight imported Electron");');
    return { directory, runtimeDirectory, executable };
}

test('Electron preflight accepts the pinned executable without importing or running Electron', t => {
    const fixture = runtimeFixture(t);
    assert.equal(checkRuntime(fixture.directory), fixture.executable);
});

test('Electron preflight rejects missing binary metadata without downloading it', t => {
    const fixture = runtimeFixture(t);
    rmSync(path.join(fixture.runtimeDirectory, 'path.txt'));
    assert.throws(() => checkRuntime(fixture.directory), error => error.code === 'ENOENT' && error.path.endsWith('path.txt'));
});

test('Electron preflight rejects stale npm packages and stale binary versions', t => {
    const packageFixture = runtimeFixture(t);
    writeFileSync(path.join(packageFixture.runtimeDirectory, 'package.json'), JSON.stringify({ version: '0.0.0' }));
    assert.throws(() => checkRuntime(packageFixture.directory), /Electron npm package 0\.0\.0 does not match pinned/);

    const binaryFixture = runtimeFixture(t);
    writeFileSync(path.join(binaryFixture.runtimeDirectory, 'dist', 'version'), '0.0.0');
    assert.throws(() => checkRuntime(binaryFixture.directory), /Electron binary 0\.0\.0 does not match pinned/);
});

test('Electron preflight rejects wrong platform paths and absent or non-executable binaries', t => {
    const wrongPath = runtimeFixture(t);
    writeFileSync(path.join(wrongPath.runtimeDirectory, 'path.txt'), '../outside');
    assert.throws(() => checkRuntime(wrongPath.directory), /does not match the supported Linux installation/);

    const missing = runtimeFixture(t);
    rmSync(missing.executable);
    assert.throws(() => checkRuntime(missing.directory), error => error.code === 'ENOENT' && error.path === missing.executable);

    const notExecutable = runtimeFixture(t);
    chmodSync(notExecutable.executable, 0o644);
    assert.throws(() => checkRuntime(notExecutable.directory), error => error.code === 'EACCES' && error.path === notExecutable.executable);
});
