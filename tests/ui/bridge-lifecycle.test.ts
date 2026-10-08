import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

class FakeWindow extends EventTarget {
    __ELECTRON__ = true;
    __PENTIMENTO_RECEIVE__?: (msg: string) => void;
    ipc?: { postMessage: (message: string) => void };
    __PENTIMENTO_IPC__?: { postMessage: (message: string) => void };
    listenerCounts = new Map<string, Set<EventListenerOrEventListenerObject>>();

    override addEventListener(
        type: string,
        listener: EventListenerOrEventListenerObject | null,
        options?: AddEventListenerOptions | boolean
    ): void {
        if (listener) {
            if (!this.listenerCounts.has(type)) {
                this.listenerCounts.set(type, new Set());
            }
            this.listenerCounts.get(type)!.add(listener);
        }
        super.addEventListener(type, listener, options);
    }

    override removeEventListener(
        type: string,
        listener: EventListenerOrEventListenerObject | null,
        options?: EventListenerOptions | boolean
    ): void {
        if (listener) {
            this.listenerCounts.get(type)?.delete(listener);
        }
        super.removeEventListener(type, listener, options);
    }

    listenerCount(type: string): number {
        return this.listenerCounts.get(type)?.size ?? 0;
    }
}

class FakeMutationObserver {
    static instances: FakeMutationObserver[] = [];

    readonly callback: MutationCallback;
    observedTarget: Node | null = null;
    disconnected = false;

    constructor(callback: MutationCallback) {
        this.callback = callback;
        FakeMutationObserver.instances.push(this);
    }

    observe(target: Node): void {
        this.observedTarget = target;
    }

    disconnect(): void {
        this.disconnected = true;
    }

    trigger(): void {
        if (!this.disconnected) {
            this.callback([], this as unknown as MutationObserver);
        }
    }

    static reset(): void {
        FakeMutationObserver.instances = [];
    }
}

function setupDom(mode: 'wasm' | 'native' = 'wasm') {
    FakeMutationObserver.reset();
    const fakeWindow = new FakeWindow();
    if (mode === 'native') {
        delete (fakeWindow as FakeWindow & { __ELECTRON__?: boolean }).__ELECTRON__;
    }

    const events: string[] = [];
    fakeWindow.addEventListener('pentimento:ui-to-bevy', (event) => {
        events.push((event as CustomEvent<string>).detail);
    });

    Object.assign(globalThis, {
        window: fakeWindow,
        document: { body: {} },
        MutationObserver: FakeMutationObserver,
    });

    return { fakeWindow, events };
}

async function importBridgeModule() {
    const url = new URL('../../ui/src/lib/bridge.ts', import.meta.url);
    url.searchParams.set('t', `${Date.now()}-${Math.random()}`);
    return import(url.href);
}

test('setupAutoMarkDirty cleans up observer and resize listener', async () => {
    const { fakeWindow, events } = setupDom();
    const { setupAutoMarkDirty } = await importBridgeModule();

    const teardown = setupAutoMarkDirty();
    const observer = FakeMutationObserver.instances[0];

    assert.equal(fakeWindow.listenerCount('resize'), 1);
    observer.trigger();
    fakeWindow.dispatchEvent(new Event('resize'));
    assert.equal(events.length, 2);

    teardown();

    assert.equal(observer.disconnected, true);
    assert.equal(fakeWindow.listenerCount('resize'), 0);

    observer.trigger();
    fakeWindow.dispatchEvent(new Event('resize'));
    assert.equal(events.length, 2);
});

test('setupAutoMarkDirty reinitialization tears down the previous registration', async () => {
    const { fakeWindow, events } = setupDom();
    const { setupAutoMarkDirty } = await importBridgeModule();

    const firstTeardown = setupAutoMarkDirty();
    const firstObserver = FakeMutationObserver.instances[0];
    const secondTeardown = setupAutoMarkDirty();
    const secondObserver = FakeMutationObserver.instances[1];

    assert.equal(firstObserver.disconnected, true);
    assert.equal(fakeWindow.listenerCount('resize'), 1);

    fakeWindow.dispatchEvent(new Event('resize'));
    assert.equal(events.length, 1);

    firstTeardown();
    assert.equal(fakeWindow.listenerCount('resize'), 1);

    secondTeardown();
    assert.equal(secondObserver.disconnected, true);
    assert.equal(fakeWindow.listenerCount('resize'), 0);
});

test('bridge.dispose clears the layout timer and removes the wasm listener', async () => {
    const { fakeWindow, events } = setupDom();
    const { bridge } = await importBridgeModule();
    const received: string[] = [];

    bridge.subscribe((message) => {
        received.push(message.type);
    });
    fakeWindow.dispatchEvent(
        new CustomEvent('pentimento:bevy-to-ui', {
            detail: JSON.stringify({ type: 'CloseMenus' }),
        })
    );

    assert.deepEqual(received, ['CloseMenus']);

    bridge.updateLayout({ regions: [] });
    bridge.dispose();

    await new Promise((resolve) => setTimeout(resolve, 25));
    fakeWindow.dispatchEvent(
        new CustomEvent('pentimento:bevy-to-ui', {
            detail: JSON.stringify({ type: 'CloseMenus' }),
        })
    );

    assert.deepEqual(received, ['CloseMenus']);
    assert.equal(events.length, 0);
});

test('bridge.dispose restores the previous native receiver', async () => {
    const previousReceiver = () => undefined;
    const { fakeWindow, events } = setupDom('native');
    fakeWindow.__PENTIMENTO_RECEIVE__ = previousReceiver;
    const { bridge } = await importBridgeModule();

    bridge.updateLayout({ regions: [] });
    bridge.dispose();

    await new Promise((resolve) => setTimeout(resolve, 25));

    assert.equal(events.length, 0);
    assert.equal(fakeWindow.__PENTIMENTO_RECEIVE__, previousReceiver);
});

test('brush bridge sends the Rust contract including unit commands and input capture', async () => {
    const { events } = setupDom();
    const { bridge } = await importBridgeModule();
    bridge.requestBrushState();
    bridge.paintCommand({ SetBrushSize: { size: 35 } });
    bridge.paintCommand({ SetBrushSpacing: { spacing: 0.4 } });
    bridge.paintCommand('Undo');
    bridge.paintCommand('ProjectToScene');
    bridge.sculptCommand({ SetTool: { tool: 'Grab' } });
    bridge.sculptCommand({ SetRadius: { radius: 1.25 } });
    bridge.sculptCommand({ SetStrength: { strength: 0.4 } });
    bridge.sculptCommand({ SetHardness: { hardness: 0.2 } });
    bridge.sculptCommand({ SetFalloff: { falloff: 'Sharp' } });
    bridge.setUiInputCapture(true);
    assert.deepEqual(events.map(event => JSON.parse(event)), [
        { type: 'RequestBrushState' },
        { type: 'PaintCommand', data: { SetBrushSize: { size: 35 } } },
        { type: 'PaintCommand', data: { SetBrushSpacing: { spacing: 0.4 } } },
        { type: 'PaintCommand', data: 'Undo' },
        { type: 'PaintCommand', data: 'ProjectToScene' },
        { type: 'SculptCommand', data: { SetTool: { tool: 'Grab' } } },
        { type: 'SculptCommand', data: { SetRadius: { radius: 1.25 } } },
        { type: 'SculptCommand', data: { SetStrength: { strength: 0.4 } } },
        { type: 'SculptCommand', data: { SetHardness: { hardness: 0.2 } } },
        { type: 'SculptCommand', data: { SetFalloff: { falloff: 'Sharp' } } },
        { type: 'SetUiInputCapture', data: { keyboard: true } },
    ]);
    bridge.dispose();
});


const nativeBootstrap = readFileSync(new URL('../../crates/webview/src/cef_bootstrap.js', import.meta.url), 'utf8');

function bootstrapNative(fakeWindow: FakeWindow, events: string[]) {
    runInNewContext(nativeBootstrap, {
        window: fakeWindow, Event,
        console: { log: (message: string) => {
            if (message.startsWith('__PENTIMENTO_IPC__:')) events.push(message.slice('__PENTIMENTO_IPC__:'.length));
        } },
    });
}

test('delayed native transport replays only latest bootstrap state after receiver readiness', async () => {
    const { fakeWindow, events } = setupDom('native');
    const { bridge } = await importBridgeModule();
    assert.equal(typeof fakeWindow.__PENTIMENTO_RECEIVE__, 'function');
    bridge.requestBrushState(); bridge.requestBrushState();
    bridge.updateLayout({ regions: [] });
    bridge.updateLayout({ regions: [{ id: 'latest', x: 1, y: 2, width: 30, height: 40, z_index: 1, accepts_keyboard: true }] });
    bridge.setUiInputCapture(true); bridge.setUiInputCapture(false);
    bridge.markDirty();
    // These must never become delayed actions when the transport appears.
    bridge.paintCommand('Undo'); bridge.paintCommand('ProjectToScene');
    bridge.sculptCommand({ SetRadius: { radius: 3 } });
    await new Promise(resolve => setTimeout(resolve, 25));
    fakeWindow.dispatchEvent(new Event('pentimento:ipc-ready'));
    assert.equal(events.length, 0); // A ready signal without a transport changes nothing.
    bootstrapNative(fakeWindow, events);
    const messages = events.map(event => JSON.parse(event));
    assert.equal(messages.filter(message => message.type === 'RequestBrushState').length, 1);
    assert.deepEqual(messages.find(message => message.type === 'LayoutUpdate').data.regions.map(region => region.id), ['latest']);
    assert.deepEqual(messages.find(message => message.type === 'SetUiInputCapture'), { type: 'SetUiInputCapture', data: { keyboard: false } });
    assert.ok(messages.every(message => ['RequestBrushState', 'LayoutUpdate', 'SetUiInputCapture', 'UiDirty'].includes(message.type)));
    events.length = 0;
    bootstrapNative(fakeWindow, events);
    assert.deepEqual(events.map(event => JSON.parse(event)), [{ type: 'UiDirty' }]);
    bridge.dispose();
});

test('CEF bootstrap waits for receiver and supports a fresh navigation document', async () => {
    const { fakeWindow, events } = setupDom('native');
    let ready = 0;
    fakeWindow.addEventListener('pentimento:ipc-ready', () => ready++);
    bootstrapNative(fakeWindow, events);
    assert.equal(ready, 0);
    const { bridge } = await importBridgeModule();
    bootstrapNative(fakeWindow, events);
    assert.equal(ready, 1);
    bridge.requestBrushState();
    assert.equal(JSON.parse(events.at(-1)!).type, 'RequestBrushState');
    bridge.dispose();

    // Navigation creates a new window and transport; no old pending state is
    // replayed into the new document. The new receiver can request fresh state.
    const next = setupDom('native');
    const { bridge: nextBridge } = await importBridgeModule();
    nextBridge.requestBrushState();
    bootstrapNative(next.fakeWindow, next.events);
    assert.equal(next.events.map(event => JSON.parse(event)).filter(message => message.type === 'RequestBrushState').length, 1);
    nextBridge.dispose();
});

test('native disposal/remount drops old bootstrap state and releases ready listener', async () => {
    const { fakeWindow, events } = setupDom('native');
    const { bridge } = await importBridgeModule();
    bridge.requestBrushState();
    bridge.updateLayout({ regions: [] });
    bridge.setUiInputCapture(true);
    bridge.dispose();
    assert.equal(fakeWindow.listenerCount('pentimento:ipc-ready'), 0);
    const { bridge: mounted } = await importBridgeModule();
    assert.equal(fakeWindow.listenerCount('pentimento:ipc-ready'), 1);
    mounted.requestBrushState();
    bootstrapNative(fakeWindow, events);
    await new Promise(resolve => setTimeout(resolve, 25));
    const messages = events.map(event => JSON.parse(event));
    assert.equal(messages.filter(message => message.type === 'RequestBrushState').length, 1);
    assert.ok(messages.every(message => message.type === 'RequestBrushState' || message.type === 'UiDirty'));
    bridge.paintCommand('ProjectToScene');
    assert.equal(events.length, messages.length);
    mounted.dispose();
    assert.equal(fakeWindow.listenerCount('pentimento:ipc-ready'), 0);
});


test('fresh direct bootstrap state supersedes older pending state before ready event', async () => {
    const { fakeWindow, events } = setupDom('native');
    const { bridge } = await importBridgeModule();
    bridge.setUiInputCapture(true);
    fakeWindow.ipc = { postMessage: message => events.push(message) };
    bridge.setUiInputCapture(false);
    fakeWindow.dispatchEvent(new Event('pentimento:ipc-ready'));
    assert.deepEqual(events.map(event => JSON.parse(event)), [
        { type: 'SetUiInputCapture', data: { keyboard: false } },
    ]);
    bridge.dispose();
});

test('continuous layout changes deliver latest rectangles without restarting the timer', async (context) => {
    const { events } = setupDom();
    const { bridge } = await importBridgeModule();
    context.mock.timers.enable({ apis: ['setTimeout'] });
    const layout = (x: number) => ({ regions: [{ id: 'brush', x, y: 0, width: 100, height: 100, z_index: 1, accepts_keyboard: true }] });
    bridge.updateLayout(layout(0));
    for (let x = 1; x <= 3; x++) {
        context.mock.timers.tick(4);
        bridge.updateLayout(layout(x));
    }
    context.mock.timers.tick(4);
    const sent = events.map(message => JSON.parse(message)).filter(message => message.type === 'LayoutUpdate');
    assert.equal(sent.length, 1);
    assert.equal(sent[0].data.regions[0].x, 3);
    bridge.updateLayout(layout(4));
    bridge.dispose();
    context.mock.timers.tick(32);
    assert.equal(events.map(message => JSON.parse(message)).filter(message => message.type === 'LayoutUpdate').length, 1);
});

test('IPC readiness preserves a newer layout waiting behind an older bootstrap report', async (context) => {
    const { fakeWindow, events } = setupDom('native');
    const { bridge } = await importBridgeModule();
    context.mock.timers.enable({ apis: ['setTimeout'] });
    const layout = (x: number) => ({ regions: [{ id: 'brush', x, y: 0, width: 100, height: 100, z_index: 1, accepts_keyboard: true }] });
    bridge.updateLayout(layout(1));
    context.mock.timers.tick(16); // Old report queued while IPC is unavailable.
    bridge.updateLayout(layout(2));
    bootstrapNative(fakeWindow, events); // Readiness flushes old report mid-timer.
    context.mock.timers.tick(16);
    const sent = events.map(message => JSON.parse(message)).filter(message => message.type === 'LayoutUpdate');
    assert.equal(sent.at(-1).data.regions[0].x, 2);
    bridge.dispose();
});
