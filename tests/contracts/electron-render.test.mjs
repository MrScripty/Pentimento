import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const { inspectScene } = createRequire(import.meta.url)('../electron/frame-evidence.cjs');
function pixels(fn) {
    const buffer = Buffer.alloc(100 * 100 * 4);
    for (let i = 0; i < 10000; i++) buffer.set(fn(i), i * 4);
    return buffer;
}
test('render evidence rejects uniform clear and grayscale gradient frames', () => {
    for (const source of [() => [80,80,80,255], i => [i%200,i%200,i%200,255]]) {
        assert.throws(() => inspectScene(pixels(source), 100, 100));
    }
});
test('render evidence rejects transparent frames and absent scene objects', () => {
    assert.throws(() => inspectScene(pixels(i => [i%255,80,20,0]), 100, 100));
    assert.throws(() => inspectScene(pixels(i => [i%200,80,20,255]), 100, 100));
});
test('render evidence recognizes all required canonical material color populations', () => {
    const source = pixels(i => {
        if (i < 1000) return [20,20,100+i%100,255];
        if (i < 2000) return [20,100+i%100,20,255];
        if (i < 3000) return [100+i%100,60,20,255];
        return [80,80,80,255];
    });
    const result = inspectScene(source, 100, 100);
    assert.equal(result.counts.red, 1000);
    assert.equal(result.counts.green, 1000);
    assert.equal(result.counts.blue, 1000);
});
