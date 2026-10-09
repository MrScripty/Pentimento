import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { EventEmitter } from 'node:events';
import { createRequire } from 'node:module';
const { createSandboxObserver } = createRequire(import.meta.url)('../electron/sandbox-observer.cjs');

function fixture(t) {
    const ipcMain = new EventEmitter();
    const registered = [], unregistered = [];
    const session = {
        registerPreloadScript(script) { registered.push(script); return 'test-observer'; },
        unregisterPreloadScript(id) { unregistered.push(id); },
    };
    const expectedUrl = 'file:///reviewed/dist/ui/index.html';
    const failures = [];
    const observer = createSandboxObserver({ ipcMain, session, expectedUrl, onFailure: error => failures.push(error) });
    t.after(() => observer.cleanup());
    const source = fs.readFileSync(registered[0].filePath, 'utf8');
    const channel = JSON.parse(source.match(/send\(("[^"]+")/)[1]);
    const frame = { url: expectedUrl, processId: 42, routingId: 3 };
    const contents = { id: 7, mainFrame: frame };
    observer.bind(contents);
    const emit = (event = { sender: contents, senderFrame: frame }, value = { sandboxed: true, contextIsolated: true }) => ipcMain.emit(channel, event, value);
    return { observer, ipcMain, channel, registered, unregistered, source, frame, contents, emit, failures };
}

test('session observer reports actual flags with production sender/frame identity', async (t) => {
    const f = fixture(t);
    assert.equal(f.registered[0].type, 'frame');
    assert.match(f.source, /process\.sandboxed === true/);
    assert.doesNotMatch(f.source, /contextBridge|window\.|globalThis\./);
    f.emit();
    assert.deepEqual(await f.observer.observation, {
        sandboxed: true, contextIsolated: true, webContentsId: 7, frameProcessId: 42,
        frameRoutingId: 3, url: f.frame.url,
    });
});

test('foreign WebContents, subframes, missing frames and wrong file URLs cannot attest production', async (t) => {
    const f = fixture(t);
    let settled = false;
    f.observer.observation.then(() => { settled = true; });
    f.emit({ sender: { ...f.contents }, senderFrame: f.frame });
    f.emit({ sender: f.contents, senderFrame: { ...f.frame } });
    f.emit({ sender: f.contents, senderFrame: null });
    const expected = f.frame.url;
    f.frame.url = 'file:///unreviewed/index.html';
    f.emit();
    await Promise.resolve();
    assert.equal(settled, false);
    f.frame.url = expected;
    f.emit();
    assert.equal((await f.observer.observation).sandboxed, true);
});

test('false process flags are reported honestly for the caller to reject', async (t) => {
    const f = fixture(t);
    f.emit(undefined, { sandboxed: false, contextIsolated: false });
    const report = await f.observer.observation;
    assert.equal(report.sandboxed, false);
    assert.equal(report.contextIsolated, false);
});

test('malformed observation from the matching renderer rejects instead of qualifying', async (t) => {
    const f = fixture(t);
    const rejected = assert.rejects(f.observer.observation, /Malformed/);
    f.emit(undefined, { sandboxed: 'true', contextIsolated: true });
    await rejected;
});

test('only the first matching observation is accepted', async (t) => {
    const f = fixture(t);
    f.emit();
    f.emit(undefined, { sandboxed: false, contextIsolated: false });
    assert.equal((await f.observer.observation).sandboxed, true);
    assert.equal(f.ipcMain.listenerCount(f.channel), 0);
});

test('cleanup unregisters only its preload and removes its temporary script/listener', (t) => {
    const f = fixture(t);
    f.observer.cleanup();
    f.observer.cleanup();
    assert.deepEqual(f.unregistered, ['test-observer']);
    assert.equal(fs.existsSync(f.registered[0].filePath), false);
    assert.equal(f.ipcMain.listenerCount(f.channel), 0);
});

test('registration failure removes the temporary script and listener', () => {
    const ipcMain = new EventEmitter();
    let filePath;
    assert.throws(() => createSandboxObserver({ ipcMain, expectedUrl: 'file:///reviewed/index.html', onFailure: () => {}, session: {
        registerPreloadScript(script) { filePath = script.filePath; throw new Error('registration failed'); },
    } }), /registration failed/);
    assert.equal(fs.existsSync(filePath), false);
    assert.equal(ipcMain.eventNames().length, 0);
});

test('observer cannot be rebound to a different WebContents', (t) => {
    const f = fixture(t);
    assert.throws(() => f.observer.bind({ id: 8 }), /already bound/);
});

test('early malformed report reaches failure handling before readiness without resolving the original promise', async (t) => {
    const f = fixture(t);
    const original = f.observer.observation;
    f.emit(undefined, { sandboxed: 'invalid', contextIsolated: true });
    await Promise.resolve();
    assert.equal(f.failures.length, 1);
    assert.match(f.failures[0].message, /Malformed/);
    assert.equal(f.observer.observation, original);
    await assert.rejects(original, /Malformed/);
});
