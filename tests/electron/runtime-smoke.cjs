// Run with the installed Electron binary. Never disable its sandbox or web security.
const { app, BrowserWindow, ipcMain, session } = require('electron');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const assert = require('node:assert/strict');
const { pathToFileURL } = require('node:url');
const { inspectScene, locateRedObject, inspectSelectionOutline } = require('./frame-evidence.cjs');
const { isFatalConsoleMessage } = require('./runtime-errors.cjs');
const { inspectUnavailableDepth } = require('./depth-evidence.cjs');
const root = path.resolve(__dirname, '../..');
const preflight = process.argv.includes('--preflight');
const phase = preflight ? 'environment' : 'production';
const evidence = process.env.PENTIMENTO_EVIDENCE_DIR || path.join(root, 'electron-runtime-evidence');
fs.mkdirSync(evidence, { recursive: true });
const result = { phase, versions: process.versions, platform: process.platform, architecture: process.arch,
    argv: process.argv, console: [], errors: [], network: [], success: false, willQuit: false };
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
function fail(error) {
    result.success = false;
    result.errors.push(String(error?.stack || error));
    save();
    console.error(error);
    app.exit(1);
}
process.on('uncaughtException', fail);
process.on('unhandledRejection', fail);
const timeout = setTimeout(() => fail(new Error(`${phase} startup did not qualify within 120 seconds`)), 120_000);
assert.equal(process.versions.electron, require('../../src-electron/package.json').devDependencies.electron);
assert.equal(process.getuid?.() === 0, false, 'runtime tests must run as non-root');
assert.equal(Boolean(process.env.VITE_DEV_SERVER_URL), false, 'production qualification must not use a dev server');
for (const flag of ['no-sandbox', 'disable-setuid-sandbox', 'disable-web-security', 'enable-unsafe-swiftshader']) {
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
        contents.on('console-message', async ({ message, level }) => {
            result.console.push({ level, message });
            if (isFatalConsoleMessage(level, message)) return fail(new Error(message));
            if (message !== 'Pentimento: WASM loaded!' || inspected) return;
            inspected = true;
            try {
                await new Promise(resolve => setTimeout(resolve, 3000));
                assert.equal(contents.getURL(), pathToFileURL(expectedDocument).href);
                const preferences = contents.getLastWebPreferences();
                assert.equal(preferences.nodeIntegration, false);
                assert.equal(preferences.contextIsolation, true);
                assert.equal(preferences.sandbox, true);
                result.preferences = { nodeIntegration: preferences.nodeIntegration, contextIsolation: preferences.contextIsolation, sandbox: preferences.sandbox, preload: preferences.preload };
                // The environment probe observes process.sandboxed inside its own
                // preload. Production's preload exposes only the runtime marker;
                // its lexical process polyfill is not an isolated-world global.
                result.renderer = await contents.executeJavaScript(`(() => {
                    const canvas = document.querySelector('#bevy-canvas');
                    return { marker: window.__ELECTRON__, process: typeof process, require: typeof require,
                        elements: document.querySelector('#app').childElementCount,
                        canvasVisible: canvas && getComputedStyle(canvas).display !== 'none',
                        canvasWidth: canvas?.width, canvasHeight: canvas?.height };
                })()`);
                assert.equal(result.renderer.marker, true);
                assert.equal(result.renderer.process, 'undefined');
                assert.equal(result.renderer.require, 'undefined');
                assert.ok(result.renderer.elements > 0);
                assert.ok(result.renderer.canvasVisible && result.renderer.canvasWidth > 0 && result.renderer.canvasHeight > 0);
                result.depthView = inspectUnavailableDepth(await contents.executeJavaScript(`(() => {
                    const button = document.querySelector('button[aria-label="Toggle depth view"]');
                    const status = document.getElementById('depth-view-status');
                    return { disabled: button?.disabled, pressed: button?.getAttribute('aria-pressed'),
                        title: button?.title, describedBy: button?.getAttribute('aria-describedby'),
                        status: status?.textContent.trim(), statusHidden: status?.hidden };
                })()`));
                const screenshot = await contents.capturePage();
                assert.equal(screenshot.isEmpty(), false);
                fs.writeFileSync(path.join(evidence, 'production.png'), screenshot.toPNG());
                result.screenshot = screenshot.getSize();
                // Hide only the DOM overlay during canvas capture, so UI cannot make a blank canvas pass.
                let selectionBefore;
                await contents.executeJavaScript("document.querySelector('#app').style.visibility = 'hidden'");
                try {
                    result.canvasFrames = [];
                    for (let frame = 0; frame < 2; frame++) {
                        await new Promise(resolve => setTimeout(resolve, 500));
                        const rect = await contents.executeJavaScript(`(() => {
                            const r = document.querySelector('#bevy-canvas').getBoundingClientRect();
                            return {x: Math.ceil(r.x + r.width * .1), y: Math.ceil(r.y + r.height * .1),
                                width: Math.floor(r.width * .8), height: Math.floor(r.height * .8)};
                        })()`);
                        const canvas = await contents.capturePage(rect);
                        assert.equal(canvas.isEmpty(), false);
                        const size = canvas.getSize();
                        const scene = inspectScene(canvas.toBitmap(), size.width, size.height);
                        fs.writeFileSync(path.join(evidence, `canvas-${frame}.png`), canvas.toPNG());
                        result.canvasFrames.push({ rect, size, ...scene });
                        selectionBefore = { canvas, bitmap: canvas.toBitmap(), rect, size };
                    }
                } finally {
                    await contents.executeJavaScript("document.querySelector('#app').style.visibility = ''");
                }
                const object = locateRedObject(selectionBefore.bitmap, selectionBefore.size.width, selectionBefore.size.height);
                const click = {
                    x: Math.round(selectionBefore.rect.x + (object.point.x + .5) * selectionBefore.rect.width / selectionBefore.size.width),
                    y: Math.round(selectionBefore.rect.y + (object.point.y + .5) * selectionBefore.rect.height / selectionBefore.size.height),
                };
                assert.equal(await contents.executeJavaScript(`document.elementFromPoint(${click.x}, ${click.y})?.id`), 'bevy-canvas',
                    'selection input must target a visible scene object, not an overlay');
                result.selection = { click, object, rect: selectionBefore.rect, size: selectionBefore.size, input: 'Electron mouse events' };
                fs.writeFileSync(path.join(evidence, 'selection-before.png'), selectionBefore.canvas.toPNG());
                win.focus();
                contents.focus();
                contents.sendInputEvent({ type: 'mouseMove', ...click });
                // Software-rendered picking needs a completed hover update before
                // press, and a separate update with the button down before release.
                await new Promise(resolve => setTimeout(resolve, 1000));
                contents.sendInputEvent({ type: 'mouseDown', button: 'left', clickCount: 1, ...click });
                await new Promise(resolve => setTimeout(resolve, 1000));
                contents.sendInputEvent({ type: 'mouseUp', button: 'left', clickCount: 1, ...click });
                // Park the pointer on the toolbar, so hover feedback cannot qualify.
                contents.sendInputEvent({ type: 'mouseMove', x: 20, y: 20 });
                await new Promise(resolve => setTimeout(resolve, 500));
                await contents.executeJavaScript("document.querySelector('#app').style.visibility = 'hidden'");
                try {
                    const deadline = Date.now() + 15_000;
                    let consecutive = 0, lastError;
                    while (Date.now() < deadline && consecutive < 2) {
                        await new Promise(resolve => setTimeout(resolve, 250));
                        const after = await contents.capturePage(selectionBefore.rect);
                        const size = after.getSize();
                        assert.deepEqual(size, selectionBefore.size);
                        fs.writeFileSync(path.join(evidence, 'selection-after.png'), after.toPNG());
                        try {
                            inspectScene(after.toBitmap(), size.width, size.height);
                            result.selection.pixels = inspectSelectionOutline(selectionBefore.bitmap, after.toBitmap(), size.width, size.height);
                            consecutive++;
                        } catch (error) {
                            lastError = error;
                            consecutive = 0;
                        }
                    }
                    assert.equal(consecutive, 2, `selection did not produce a stable real outline: ${lastError?.message}`);
                    result.selection.stableFrames = consecutive;
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
