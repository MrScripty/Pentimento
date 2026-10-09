import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';

const ts = createRequire(import.meta.url)('typescript');
const source = ts.transpileModule(readFileSync(new URL('../../ui/src/lib/layout.ts', import.meta.url), 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
}).outputText;

function fixture() {
    const layouts = [], capture = [], frames = new Map(), timers = new Map(), microtasks = [];
    let id = 0, observer, rect = { x: 600, y: 40, width: 200, height: 560 };
    const element = { dataset: { uiRegion: 'brush-panel' }, getBoundingClientRect: () => rect };
    const root = { querySelectorAll: () => [element], contains: item => item === element };
    const window = new EventTarget();
    const document = Object.assign(new EventTarget(), { activeElement: element });
    const exports = {};
    runInNewContext(source, {
        exports, window, document,
        require: () => ({ bridge: { updateLayout: layout => layouts.push(layout), setUiInputCapture: value => capture.push(value) } }),
        requestAnimationFrame: callback => { frames.set(++id, callback); return id; },
        cancelAnimationFrame: handle => frames.delete(handle),
        setTimeout: callback => { timers.set(++id, callback); return id; },
        clearTimeout: handle => timers.delete(handle),
        queueMicrotask: callback => microtasks.push(callback),
        MutationObserver: class {
            constructor(callback) { observer = this; this.callback = callback; }
            observe() {}
            disconnect() { this.disconnected = true; }
        }
    });
    const cleanup = exports.setupInputLayout(root);
    const run = pending => { for (const [handle, callback] of [...pending]) { if (pending.delete(handle)) callback(); } };
    return { layouts, capture, frames, timers, microtasks, observer, window, document, cleanup,
        mutate: next => { rect = next; observer.callback(); }, run };
}

test('layout reports initial and changed panels even when RAF never runs', () => {
    const f = fixture();
    assert.equal(f.layouts.length, 1);
    assert.equal(f.layouts[0].regions[0].x, 600);
    f.mutate({ x: 450, y: 40, width: 200, height: 560 });
    assert.equal(f.timers.size, 1);
    f.run(f.timers); // Deliberately leave animation frames stalled.
    assert.equal(f.layouts.length, 2);
    assert.equal(f.layouts[1].regions[0].x, 450);
    assert.equal(f.frames.size, 0);
    f.cleanup();
});

test('RAF and fallback share one job and deliver the latest rectangles', () => {
    for (const first of ['frames', 'timers']) {
        const f = fixture();
        f.mutate({ x: 500, y: 40, width: 200, height: 560 });
        f.mutate({ x: 400, y: 40, width: 200, height: 560 });
        assert.equal(f.frames.size, 1);
        assert.equal(f.timers.size, 1);
        f.run(f[first]);
        f.run(f.frames); f.run(f.timers);
        assert.equal(f.layouts.length, 2);
        assert.equal(f.layouts[1].regions[0].x, 400);
        f.cleanup();
    }
});

test('layout cleanup cancels scheduled work and guards queued focus callbacks', () => {
    const f = fixture();
    f.mutate({ x: 500, y: 40, width: 200, height: 560 });
    const lateFrame = [...f.frames.values()][0], lateTimer = [...f.timers.values()][0];
    f.document.dispatchEvent(new Event('focusin'));
    f.cleanup();
    assert.equal(f.observer.disconnected, true);
    assert.equal(f.frames.size + f.timers.size, 0);
    lateFrame(); lateTimer();
    f.microtasks.forEach(callback => callback());
    f.window.dispatchEvent(new Event('resize'));
    f.observer.callback(); // Already queued observer notification after teardown.
    assert.equal(f.layouts.length, 1);
    assert.equal(f.capture.length, 0);
    assert.equal(f.frames.size + f.timers.size, 0);
});
