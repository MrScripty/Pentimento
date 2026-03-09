import test from 'node:test';
import assert from 'node:assert/strict';

class FakeWindow extends EventTarget {
    __ELECTRON__ = true;
    __PENTIMENTO_RECEIVE__?: (msg: string) => void;
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
