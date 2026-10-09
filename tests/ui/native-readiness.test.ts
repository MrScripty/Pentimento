import test from 'node:test';
import assert from 'node:assert/strict';
import { cefFramebufferReceipt, startedStroke, completedStroke, assertAcceptedStroke, waitForSculptPresentation, parkedPointerFrames, holdNativeKey, tapNativeShortcut, assertNativeClickBounds, assertPaintedUiRegion, cefRenderingArguments } from '../native/readiness.mjs';

test('open modal capture requires four UI-owned idle frames without relaxing scene capture', () => {
    const frame = (blocked = true, latched = false, received = true) =>
        `Native capture after batch: last (20.0, 1050.0) blocked=${blocked} latched=${latched} layout_received=${received}\n`;
    const modal = frame().repeat(4);
    assert.equal(parkedPointerFrames(modal, 20, 1050), null);
    assert.deepEqual(parkedPointerFrames(modal, 20, 1050, { uiOwned: true }), { x: 20, y: 1050, consecutive_updates: 4 });
    assert.equal(parkedPointerFrames(frame().repeat(3), 20, 1050, { uiOwned: true }), null);
    assert.equal(parkedPointerFrames(frame(false).repeat(4), 20, 1050, { uiOwned: true }), null);
    assert.equal(parkedPointerFrames(frame(true, true).repeat(4), 20, 1050, { uiOwned: true }), null);
    assert.equal(parkedPointerFrames(frame(true, false, false).repeat(4), 20, 1050, { uiOwned: true }), null);
    assert.equal(parkedPointerFrames(modal, 21, 1050, { uiOwned: true }), null);
});

test('DOM, startup and stroke Start do not imply a painted CEF framebuffer', () => {
    assert.equal(cefFramebufferReceipt('Frontend initialized (Cef mode)\nDOM ready'), null);
    assert.equal(cefFramebufferReceipt('CEF webview ready'), null);
    assert.equal(cefFramebufferReceipt('CEF webview ready\nFirst capture (Cef mode): 100x100, non-transparent pixels: 100'), null);
    assert.equal(cefFramebufferReceipt('First painted capture (Cef mode): 100x100, non-transparent pixels: 100'), null);
    assert.equal(cefFramebufferReceipt('CEF webview ready\nFirst painted capture (Cef mode): 100x100, non-transparent pixels: 0'), null);
    assert.deepEqual(cefFramebufferReceipt('CEF webview ready\nFirst painted capture (Cef mode): 100x100, non-transparent pixels: 100'), { width: 100, height: 100, painted_pixels: 100 });
});

test('completion waits for the exact transaction after finalization', () => {
    assert.deepEqual(startedStroke('Sculpt stroke started: id=7, pos=Vec3()'), { id: '7' });
    assert.equal(completedStroke('Sculpt stroke started: id=7,\nSculpt stroke ended: id=7', '7'), null);
    assert.equal(completedStroke('Sculpt stroke completed: id=70, outcome=accepted, faces=224', '7'), null);
    for (const outcome of ['accepted', 'no_change', 'rejected', 'cancelled']) {
        assert.deepEqual(completedStroke(`Sculpt stroke completed: id=7, outcome=${outcome}, faces=224`, '7'), { id: '7', outcome, faces: 224 });
    }
});

test('invisible controls and sparse world lines fail native UI framebuffer evidence', () => {
    const frame = { pixels: new Uint8Array(100 * 100 * 3) };
    const rect = { x: 0, y: 0, width: 100, height: 100 };
    assert.throws(() => assertPaintedUiRegion(frame, 100, 100, rect, 'toolbar'), /does not show/);
    for (let x = 0; x < 100; x++) frame.pixels[x * 3] = 255;
    assert.throws(() => assertPaintedUiRegion(frame, 100, 100, rect, 'toolbar'), /does not show/);
    frame.pixels.fill(30);
    assert.equal(assertPaintedUiRegion(frame, 100, 100, rect, 'toolbar').painted_pixels, 10000);
});

test('software OSR configuration is bounded to rendering flags only', () => {
    assert.deepEqual(cefRenderingArguments('default'), []);
    assert.deepEqual(cefRenderingArguments('software'), ['--disable-gpu', '--disable-gpu-compositing']);
    assert.throws(() => cefRenderingArguments('--no-sandbox'), /Unsupported/);
    assert.ok(cefRenderingArguments('software').every(flag => !/sandbox|security|certificate|origin|network/.test(flag)));
});

test('visible sculpt and a new branch require accepted finalization', () => {
    const accepted = { id: '7', outcome: 'accepted', faces: 224 };
    assert.equal(assertAcceptedStroke(accepted, 'visible sculpt'), accepted);
    for (const outcome of ['no_change', 'rejected', 'cancelled']) {
        assert.throws(() => assertAcceptedStroke({ ...accepted, outcome }, 'visible sculpt'), /must be accepted/);
    }
});

test('transparent initial capture waits for the first nonempty uploaded capture', () => {
    const initial = 'CEF webview ready\nFirst capture (Cef mode): 1920x1080, non-transparent pixels: 0';
    assert.equal(cefFramebufferReceipt(initial), null);
    assert.equal(cefFramebufferReceipt(`${initial}\nFirst painted capture (Cef mode): 1920x1080, non-transparent pixels: 0`), null);
    assert.deepEqual(cefFramebufferReceipt(`${initial}\nFirst painted capture (Cef mode): 1920x1080, non-transparent pixels: 410000`), { width: 1920, height: 1080, painted_pixels: 410000 });
});


test('presentation waits for backend history and native pixels under the original threshold', async () => {
    let attempt = 0;
    const wait = async (observe, _description, timeout) => {
        assert.equal(timeout, 30000);
        for (attempt = 0; attempt < 4; attempt++) {
            const result = await observe();
            if (result) return result;
        }
        throw new Error('deadline reached');
    };
    let captures = 0;
    const result = await waitForSculptPresentation(wait, {
        historyReady: async () => attempt > 0,
        capture: async () => { captures++; return { sequence: attempt }; },
        measure: frame => ({ changed_pixels: frame.sequence === 1 ? 25 : 26 }),
        description: 'accepted stroke 7',
    });
    assert.equal(result.frame.sequence, 2);
    assert.equal(captures, 2);
    for (const ready of [false, true]) {
        await assert.rejects(waitForSculptPresentation(wait, {
            historyReady: async () => ready,
            capture: async () => ({}),
            measure: () => ({ changed_pixels: ready ? 0 : 100 }),
            description: 'never presented',
        }), /deadline reached/);
    }
});


test('capture waits for fresh consecutive parked pointer updates through the render pipeline', () => {
    const receipt = 'Native capture after batch: last (20.0, 1050.0) blocked=false latched=false layout_received=true\n';
    for (let count = 0; count < 4; count++) assert.equal(parkedPointerFrames(receipt.repeat(count), 20, 1050), null);
    assert.equal(parkedPointerFrames(receipt.repeat(3) + receipt.trimEnd(), 20, 1050), null);
    const oldReceipts = receipt.repeat(4);
    assert.equal(parkedPointerFrames((oldReceipts + receipt.repeat(3)).slice(oldReceipts.length), 20, 1050), null);
    assert.deepEqual(parkedPointerFrames(receipt.repeat(4), 20, 1050), { x: 20, y: 1050, consecutive_updates: 4 });
    for (const interruption of [receipt.replace('20.0', '1215.0'), receipt.replace('blocked=false', 'blocked=true'), receipt.replace('latched=false', 'latched=true'), receipt.replace('layout_received=true', 'layout_received=false')]) {
        assert.equal(parkedPointerFrames(receipt.repeat(4) + interruption + receipt.repeat(3), 20, 1050), null);
        assert.ok(parkedPointerFrames(receipt.repeat(4) + interruption + receipt.repeat(4), 20, 1050));
    }
});


test('native chord holds through a fresh receipt and releases before settling', async () => {
    const calls = [];
    let log = 'Exited sculpt mode\n';
    const offset = log.length;
    await holdNativeKey('ctrl+Tab', () => log.slice(offset).includes('Exited sculpt mode'), 'exit', {
        send: (action, value) => calls.push([action, value]),
        pause: async ms => calls.push(['pause', ms]),
        wait: async observed => {
            assert.equal(observed(), false, 'an old transition is not an acknowledgement');
            assert.deepEqual(calls, [['keydown', 'ctrl+Tab'], ['pause', 200]]);
            log += 'Exited sculpt mode\n';
            assert.equal(observed(), true);
            calls.push(['observed']);
        },
        releaseFailed: () => assert.fail('unexpected release error'),
    });
    assert.deepEqual(calls, [['keydown', 'ctrl+Tab'], ['pause', 200], ['observed'], ['keyup', 'ctrl+Tab'], ['pause', 350]]);
});

test('native chord releases in finally and preserves the original timeout', async () => {
    const calls = [];
    const timeout = new Error('Timed out: exit receipt');
    for (const releaseThrows of [false, true]) {
        calls.length = 0;
        await assert.rejects(holdNativeKey('ctrl+Tab', () => false, 'exit', {
            send: action => { calls.push(action); if (action === 'keyup' && releaseThrows) throw new Error('release failed'); },
            pause: async () => {},
            wait: async () => { throw timeout; },
            releaseFailed: () => calls.push('release failure recorded'),
        }), error => error === timeout);
        assert.deepEqual(calls, releaseThrows ? ['keydown', 'keyup', 'release failure recorded'] : ['keydown', 'keyup']);
    }
});


test('native clicks reject offscreen targets instead of letting X11 clamp to another item', () => {
    assert.doesNotThrow(() => assertNativeClickBounds({ x: 35, y: 700, width: 150, height: 34 }, 1920, 1080));
    for (const rect of [{ x: 35, y: 1300, width: 150, height: 34 }, { x: 1900, y: 100, width: 150, height: 34 }, { x: -1, y: 0, width: 150, height: 34 }]) {
        assert.throws(() => assertNativeClickBounds(rect, 1920, 1080), /outside the viewport/);
    }
});


test('Ctrl+Tab emits one trigger tap and retains Ctrl through backend acknowledgement', async () => {
    const calls = [];
    await tapNativeShortcut('ctrl', 'Tab', () => true, 'exit', {
        send: (action, key) => calls.push(`${action}:${key}`),
        pause: async () => {},
        wait: async observed => {
            assert.deepEqual(calls, ['keydown:ctrl', 'keydown:Tab', 'keyup:Tab']);
            assert.equal(observed(), true);
            calls.push('acknowledged');
        },
        releaseFailed: () => assert.fail('unexpected release failure'),
    });
    assert.deepEqual(calls, ['keydown:ctrl', 'keydown:Tab', 'keyup:Tab', 'acknowledged', 'keyup:ctrl']);
});

test('Ctrl+Shift+Z taps once and releases all owned modifiers on acknowledgement or failure', async () => {
    for (const fail of [false, true]) {
        const calls: string[] = [];
        const receiptFailure = new Error('receipt failed');
        const operation = tapNativeShortcut(['ctrl', 'shift'], 'z', () => !fail, 'redo', {
            send: (action: string, key: string) => calls.push(`${action}:${key}`),
            pause: async () => {},
            wait: async () => {
                assert.deepEqual(calls, ['keydown:ctrl', 'keydown:shift', 'keydown:z', 'keyup:z']);
                if (fail) throw receiptFailure;
            },
            releaseFailed: () => assert.fail('unexpected release failure'),
        });
        if (fail) await assert.rejects(operation, error => error === receiptFailure);
        else await operation;
        assert.deepEqual(calls, ['keydown:ctrl', 'keydown:shift', 'keydown:z', 'keyup:z', 'keyup:shift', 'keyup:ctrl']);
    }
});

test('every shortcut failure path attempts owned key release without hiding the original error', async () => {
    for (const failure of ['modifier-down', 'trigger-down', 'trigger-up', 'receipt', 'modifier-up', 'receipt-and-modifier-up']) {
        const calls = [];
        const primary = new Error(failure);
        await assert.rejects(tapNativeShortcut('ctrl', 'Tab', () => false, 'exit', {
            send: (action, key) => {
                const event = `${action}:${key}`;
                calls.push(event);
                if ((failure === 'modifier-down' && event === 'keydown:ctrl')
                    || (failure === 'trigger-down' && event === 'keydown:Tab')
                    || (failure === 'trigger-up' && event === 'keyup:Tab')
                    || (failure === 'modifier-up' && event === 'keyup:ctrl')) throw primary;
                if (failure === 'receipt-and-modifier-up' && event === 'keyup:ctrl') throw new Error('cleanup failure');
            },
            pause: async () => {},
            wait: async () => { if (failure.startsWith('receipt')) throw primary; },
            releaseFailed: () => calls.push('cleanup failure recorded'),
        }), error => error === primary);
        const releaseCtrl = calls.indexOf('keyup:ctrl');
        assert.ok(releaseCtrl >= 0, failure);
        if (failure !== 'modifier-down') {
            assert.ok(calls.indexOf('keyup:Tab') >= 0 && releaseCtrl > calls.indexOf('keyup:Tab'), failure);
            assert.equal(calls.filter(call => call === 'keydown:Tab').length, 1, failure);
        }
        if (failure === 'receipt-and-modifier-up') assert.ok(calls.includes('cleanup failure recorded'));
    }
});
