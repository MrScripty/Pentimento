import test from 'node:test';
import assert from 'node:assert/strict';
import { cefFramebufferReceipt, startedStroke, completedStroke, assertAcceptedStroke, waitForSculptPresentation, assertPaintedUiRegion, cefRenderingArguments } from '../native/readiness.mjs';

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
