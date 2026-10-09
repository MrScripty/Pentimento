import test from 'node:test';
import assert from 'node:assert/strict';
import { connectDepthView, initialDepthViewState } from '../../ui/src/lib/depth-view.ts';
import type { BevyToUi, DepthViewState } from '../../ui/src/lib/types.ts';

function setup(initial?: DepthViewState) {
    const handlers = new Set<(message: BevyToUi) => void>();
    const requests: boolean[] = [];
    const states: DepthViewState[] = [];
    let queries = 0;
    const emit = (message: BevyToUi) => handlers.forEach((handler) => handler(message));
    const bridge = {
        subscribe(handler: (message: BevyToUi) => void) {
            handlers.add(handler);
            return () => { handlers.delete(handler); };
        },
        getDepthViewState() {
            queries += 1;
            if (initial) emit({ type: 'DepthViewState', data: initial });
        },
        setDepthView(enabled: boolean) { requests.push(enabled); },
    };
    const controller = connectDepthView(bridge, (state) => states.push(state));
    return { bridge, controller, handlers, requests, states, emit, queries: () => queries };
}

test('depth view is disabled until the backend reports capability', () => {
    const { controller, requests, states, queries } = setup();
    assert.deepEqual(states, [initialDepthViewState()]);
    assert.equal(queries(), 1);
    controller.toggle();
    assert.deepEqual(requests, []);
    controller.dispose();
});

test('initial query recovers a late mount and subscribes before a synchronous reply', () => {
    const initial = { available: true, enabled: true, reason: null };
    const { controller, states, requests } = setup(initial);
    assert.deepEqual(states.at(-1), initial);
    controller.toggle();
    assert.deepEqual(requests, [false]);
    assert.deepEqual(states.at(-1), initial, 'request must not optimistically disable');
    controller.dispose();
});

test('unavailable backends expose the reason and cannot send a toggle', () => {
    const initial = {
        available: false,
        enabled: false,
        reason: 'Depth view is unavailable with the OpenGL/WebGL renderer.',
    };
    const { controller, states, requests } = setup(initial);
    assert.deepEqual(states.at(-1), initial);
    controller.toggle();
    controller.toggle();
    assert.deepEqual(requests, []);
    controller.dispose();
});

test('repeated clicks wait for authoritative enabled and disabled acknowledgements', () => {
    const initial = { available: true, enabled: false, reason: null };
    const { controller, states, requests, emit } = setup(initial);
    controller.toggle();
    controller.toggle();
    assert.deepEqual(requests, [true, true]);
    assert.deepEqual(states.at(-1), initial, 'requests must not optimistically enable');
    emit({ type: 'DepthViewState', data: { ...initial, enabled: true } });
    controller.toggle();
    assert.deepEqual(requests, [true, true, false]);
    assert.equal(states.at(-1)?.enabled, true);
    emit({ type: 'DepthViewState', data: initial });
    assert.equal(states.at(-1)?.enabled, false);
    controller.dispose();
});

test('rejection exposes its reason and the following state disables future requests', () => {
    const { controller, states, requests, emit } = setup({ available: true, enabled: false, reason: null });
    controller.toggle();
    emit({ type: 'DepthViewRejected', data: { reason: 'Renderer does not support depth view.' } });
    assert.equal(states.at(-1)?.enabled, false);
    assert.equal(states.at(-1)?.reason, 'Renderer does not support depth view.');
    emit({
        type: 'DepthViewState',
        data: { available: false, enabled: false, reason: 'Renderer does not support depth view.' },
    });
    controller.toggle();
    assert.deepEqual(requests, [true]);
    controller.dispose();
});

test('unavailable state never displays enabled and unrelated messages do not change it', () => {
    const { controller, states, emit } = setup({ available: false, enabled: true, reason: null });
    assert.equal(states.at(-1)?.enabled, false);
    emit({ type: 'CloseMenus' });
    assert.equal(states.length, 2);
    controller.dispose();
});

test('unmount removes the subscription and remount re-queries without stale state', () => {
    const { bridge, controller, handlers, states, requests, emit, queries } = setup();
    const oldHandler = [...handlers][0];
    controller.dispose();
    assert.equal(handlers.size, 0);
    const stateMessage: BevyToUi = { type: 'DepthViewState', data: { available: true, enabled: true, reason: null } };
    emit(stateMessage);
    oldHandler(stateMessage);
    controller.toggle();
    assert.equal(states.length, 1);
    assert.deepEqual(requests, []);

    const remountedStates: DepthViewState[] = [];
    const remounted = connectDepthView(bridge, (state) => remountedStates.push(state));
    assert.equal(queries(), 2);
    assert.deepEqual(remountedStates, [initialDepthViewState()]);
    assert.equal(handlers.size, 1);
    remounted.dispose();
});
