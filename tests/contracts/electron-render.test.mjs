import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const { inspectScene, measureScene } = createRequire(import.meta.url)('../electron/frame-evidence.cjs');
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

test('pending scene evidence keeps statistics and reasons without accepting a blank frame', () => {
    const report = measureScene(pixels(() => [80,80,80,255]), 100, 100);
    assert.equal(report.ready, false);
    assert.equal(report.colors, 1);
    assert.equal(report.variedFraction, 0);
    assert.equal(report.counts.neutral, 10000);
    assert.ok(report.pending.includes('missing canonical red object pixels'));
    assert.throws(() => measureScene(Buffer.alloc(3), 100, 100));
});


test('software rendering opt-in is confined to CI production qualification', async () => {
    const { readFile } = await import('node:fs/promises');
    const read = name => readFile(new URL('../../' + name, import.meta.url), 'utf8');
    const workflow = await read('.github/workflows/electron-runtime.yml');
    const production = workflow.split('\n').find(line => line.includes('supervise.py production'));
    const environment = workflow.split('\n').find(line => line.includes('supervise.py environment'));
    for (const flag of ['--use-gl=angle', '--use-angle=swiftshader-webgl', '--enable-unsafe-swiftshader']) {
        assert.ok(production.includes(flag));
        assert.ok(!environment.includes(flag));
        assert.ok(!(await read('src-electron/main.ts')).includes(flag));
        assert.ok(!(await read('launcher.sh')).includes(flag));
    }
    for (const flag of ['--no-sandbox', '--disable-setuid-sandbox', '--disable-web-security']) {
        assert.ok(!production.includes(flag));
    }
});
