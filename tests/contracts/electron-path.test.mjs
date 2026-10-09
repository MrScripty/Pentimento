import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const source = fs.readFileSync(path.join(root, 'src-electron/main.ts'), 'utf8');
const compiled = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020, esModuleInterop: true },
}).outputText;

function runCompiledMain(environment) {
    const observed = {};
    class BrowserWindow {
        constructor(options) {
            observed.options = options;
            this.webContents = { openDevTools: () => { observed.devTools = true; } };
        }
        loadFile(file) { observed.file = file; }
        loadURL(url) { observed.url = url; }
        on() {}
    }
    const app = { whenReady: () => ({ then: callback => callback() }), on() {}, quit() {} };
    vm.runInNewContext(compiled, {
        require: (name) => {
            if (name === 'electron') return { app, BrowserWindow };
            if (name === 'path') return path;
            throw new Error(`Unexpected main-process import: ${name}`);
        },
        exports: {},
        __dirname: path.join(root, 'src-electron/dist'),
        process: { env: environment },
    }, { filename: 'compiled-electron-main.js' });
    return observed;
}

test('compiled production main loads canonical root dist/ui with its compiled preload', () => {
    const observed = runCompiledMain({});
    assert.equal(observed.file, path.join(root, 'dist/ui/index.html'));
    assert.equal(observed.options.webPreferences.preload, path.join(root, 'src-electron/dist/preload.js'));
    assert.equal(observed.options.webPreferences.nodeIntegration, false);
    assert.equal(observed.options.webPreferences.contextIsolation, true);
    assert.equal(observed.url, undefined);
    assert.equal(observed.devTools, undefined);
});

test('compiled main retains explicit development-server behavior and preload location', () => {
    const observed = runCompiledMain({ VITE_DEV_SERVER_URL: 'http://127.0.0.1:5173' });
    assert.equal(observed.url, 'http://127.0.0.1:5173');
    assert.equal(observed.devTools, true);
    assert.equal(observed.file, undefined);
    assert.equal(observed.options.webPreferences.preload, path.join(root, 'src-electron/dist/preload.js'));
});
