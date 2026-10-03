// Run with the installed Electron binary. Never disable its sandbox or web security.
const { app, BrowserWindow, ipcMain, session } = require('electron');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const assert = require('node:assert/strict');
const { pathToFileURL } = require('node:url');
const { inspectScene, measureScene } = require('./frame-evidence.cjs');
const root = path.resolve(__dirname, '../..');
const preflight = process.argv.includes('--preflight');
const phase = preflight ? 'environment' : 'production';
const evidence = path.join(root, 'electron-runtime-evidence');
fs.mkdirSync(evidence, { recursive: true });
const result = { phase, electron: process.versions.electron, console: [], errors: [], network: [], success: false, willQuit: false };
const save = () => fs.writeFileSync(path.join(evidence, `${phase}.json`), JSON.stringify(result, null, 2) + '\n');
async function prepareShutdown() {
    const channel = process.env.PENTIMENTO_SHUTDOWN_CHANNEL;
    assert.ok(channel, 'runtime must run through the owned-process supervisor');
    fs.writeFileSync(path.join(channel, 'ready'), String(process.pid));
    const deadline = Date.now() + 5000;
    while (!fs.existsSync(path.join(channel, 'ack'))) {
        assert.ok(Date.now() < deadline, 'supervisor did not capture descendants');
        await new Promise(resolve => setTimeout(resolve, 20));
    }
}
let lastCanvasCapture;
function fail(error) {
    result.errors.push(String(error?.stack || error));
    try {
        if (lastCanvasCapture && !lastCanvasCapture.isEmpty()) {
            fs.writeFileSync(path.join(evidence, 'last-canvas.png'), lastCanvasCapture.toPNG());
        }
    } catch (captureError) {
        result.errors.push(`Could not preserve last canvas: ${captureError}`);
    }
    save();
    console.error(error);
    app.exit(1);
}
process.on('uncaughtException', fail);
process.on('unhandledRejection', fail);
const timeout = setTimeout(() => fail(new Error(`${phase} startup did not qualify within 120 seconds`)), 120_000);
assert.equal(process.versions.electron, '44.5.1');
assert.equal(process.getuid?.() === 0, false, 'runtime tests must run as non-root');
assert.equal(Boolean(process.env.VITE_DEV_SERVER_URL), false, 'production qualification must not use a dev server');
for (const flag of ['no-sandbox', 'disable-setuid-sandbox', 'disable-web-security']) {
    assert.equal(app.commandLine.hasSwitch(flag), false, `forbidden test flag: ${flag}`);
}
app.on('will-quit', () => {
    clearTimeout(timeout);
    result.willQuit = true;
    save();
});

if (preflight) {
    const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'pentimento-sandbox-probe-'));
    const preload = path.join(temporary, 'preload.cjs');
    fs.writeFileSync(preload, "require('electron').ipcRenderer.send('sandbox-probe', {sandboxed: process.sandboxed, contextIsolated: process.contextIsolated});\n");
    app.on('quit', () => fs.rmSync(temporary, { recursive: true, force: true }));
    app.whenReady().then(async () => {
        const reported = new Promise((resolve) => ipcMain.once('sandbox-probe', (_event, value) => resolve(value)));
        const win = new BrowserWindow({ show: false, webPreferences: { preload, sandbox: true, contextIsolation: true, nodeIntegration: false } });
        await win.loadURL('data:text/html,<title>Pentimento sandbox environment probe</title>');
        result.sandbox = await reported;
        assert.equal(result.sandbox.sandboxed, true);
        assert.equal(result.sandbox.contextIsolated, true);
        const privileges = await win.webContents.executeJavaScript('({process: typeof process, require: typeof require})');
        assert.deepEqual(privileges, { process: 'undefined', require: 'undefined' });
        result.success = true;
        await prepareShutdown();
        win.close();
        app.quit();
    }).catch(fail);
} else {
    const expectedDocument = path.join(root, 'dist/ui/index.html');
    for (const file of ['src-electron/dist/main.js', 'src-electron/dist/preload.js', 'dist/ui/index.html', 'dist/ui/wasm/pentimento_wasm.js', 'dist/ui/wasm/pentimento_wasm_bg.wasm']) {
        assert.equal(fs.statSync(path.join(root, file)).isFile(), true, `missing real production artifact: ${file}`);
    }
    app.on('browser-window-created', (_event, win) => {
        const contents = win.webContents;
        contents.on('did-fail-load', (_event, code, description, url, mainFrame) => {
            if (mainFrame) fail(new Error(`production load failed ${code}: ${description} (${url})`));
        });
        contents.on('preload-error', (_event, preload, error) => fail(new Error(`preload ${preload}: ${error}`)));
        contents.on('render-process-gone', (_event, details) => fail(new Error(`renderer terminated: ${JSON.stringify(details)}`)));
        let inspected = false;
        contents.on('console-message', async (_event, details, oldMessage) => {
            const message = typeof details === 'object' ? details.message : oldMessage;
            const level = typeof details === 'object' ? details.level : details;
            result.console.push({ level, message });
            if (level === 'error' || level === 3 || /panicked at|unhandled rejection|WebGL.*context lost/i.test(String(message))) return fail(new Error(message));
            if (message !== 'Pentimento: WASM loaded!' || inspected) return;
            inspected = true;
            try {
                // Backend readiness is asynchronous; the existing overall timeout bounds this wait.
                // Keep the latest observed startup state in failure evidence.
                while (true) {
                    result.depthReadiness = await contents.executeJavaScript(`(() => {
                        const control = document.querySelector('[aria-label="Toggle depth view"]');
                        return control ? {disabled: control.disabled, reason: control.title} : null;
                    })()`);
                    if (result.depthReadiness && (!result.depthReadiness.disabled ||
                        /unavailable on WebGL\/OpenGL/.test(result.depthReadiness.reason))) break;
                    await new Promise(resolve => setTimeout(resolve, 100));
                }
                assert.equal(contents.getURL(), pathToFileURL(expectedDocument).href);
                const preferences = contents.getLastWebPreferences();
                assert.equal(preferences.nodeIntegration, false);
                assert.equal(preferences.contextIsolation, true);
                assert.equal(preferences.sandbox, true);
                result.preferences = { nodeIntegration: preferences.nodeIntegration, contextIsolation: preferences.contextIsolation, sandbox: preferences.sandbox, preload: preferences.preload };
                result.sandbox = await contents.executeJavaScriptInIsolatedWorld(999, [{ code: '({sandboxed: process.sandboxed, contextIsolated: process.contextIsolated})' }]);
                assert.equal(result.sandbox.sandboxed, true);
                assert.equal(result.sandbox.contextIsolated, true);
                result.renderer = await contents.executeJavaScript(`(() => {
                    const canvas = document.querySelector('#bevy-canvas');
                    const depth = document.querySelector('[aria-label="Toggle depth view"]');
                    return { marker: window.__ELECTRON__, process: typeof process, require: typeof require,
                        elements: document.querySelector('#app').childElementCount,
                        canvasVisible: canvas && getComputedStyle(canvas).display !== 'none',
                        canvasWidth: canvas?.width, canvasHeight: canvas?.height,
                        depth: {disabled: depth?.disabled, pressed: depth?.getAttribute('aria-pressed'), reason: depth?.title} };
                })()`);
                assert.equal(result.renderer.marker, true);
                assert.equal(result.renderer.process, 'undefined');
                assert.equal(result.renderer.require, 'undefined');
                assert.equal(result.renderer.depth.disabled, true);
                assert.equal(result.renderer.depth.pressed, 'false');
                assert.match(result.renderer.depth.reason, /unavailable on WebGL\/OpenGL/);
                assert.ok(result.renderer.elements > 0);
                assert.ok(result.renderer.canvasVisible && result.renderer.canvasWidth > 0 && result.renderer.canvasHeight > 0);
                const screenshot = await contents.capturePage();
                assert.equal(screenshot.isEmpty(), false);
                fs.writeFileSync(path.join(evidence, 'production.png'), screenshot.toPNG());
                result.screenshot = screenshot.getSize();
                // Hide only the DOM overlay during canvas capture, so UI cannot make a blank canvas pass.
                await contents.executeJavaScript("document.querySelector('#app').style.visibility = 'hidden'");
                try {
                    result.canvasFrames = [];
                    for (let frame = 0; frame < 2; frame++) {
                        let canvas, size, rect, scene;
                        // Capability can arrive before the first rendered frame. Poll the same
                        // scene predicate until ready; renderer/load errors still fail immediately.
                        while (true) {
                            await new Promise(resolve => setTimeout(resolve, 100));
                            rect = await contents.executeJavaScript(`(() => {
                                const r = document.querySelector('#bevy-canvas').getBoundingClientRect();
                                return {x: Math.ceil(r.x + r.width * .1), y: Math.ceil(r.y + r.height * .1),
                                    width: Math.floor(r.width * .8), height: Math.floor(r.height * .8)};
                            })()`);
                            canvas = await contents.capturePage(rect);
                            lastCanvasCapture = canvas;
                            size = canvas.getSize();
                            if (canvas.isEmpty()) {
                                result.lastFrame = { frame, rect, size, ready: false, pending: ['empty capture'] };
                                continue;
                            }
                            const bitmap = canvas.toBitmap();
                            result.lastFrame = { frame, rect, size, ...measureScene(bitmap, size.width, size.height) };
                            if (!result.lastFrame.ready) continue;
                            scene = inspectScene(bitmap, size.width, size.height);
                            break;
                        }
                        fs.writeFileSync(path.join(evidence, `canvas-${frame}.png`), canvas.toPNG());
                        result.canvasFrames.push({ rect, size, ...scene });
                    }
                } finally {
                    await contents.executeJavaScript("document.querySelector('#app').style.visibility = ''");
                }
                result.document = contents.getURL();
                result.wasmInitialized = true;
                result.success = true;
                save();
                await prepareShutdown();
                win.close();
            } catch (error) { fail(error); }
        });
    });
    // Prove the canonical production layout is self-contained once installed.
    app.whenReady().then(() => {
        session.defaultSession.webRequest.onBeforeRequest({ urls: ['http://*/*', 'https://*/*'] }, (details, callback) => {
            result.network.push(details.url);
            callback({ cancel: true });
        });
    });
    require(path.join(root, 'src-electron/dist/main.js'));
}
