import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const { inspectScene, measureScene, locateRedObject, inspectSelectionOutline } = createRequire(import.meta.url)('../electron/frame-evidence.cjs');
const { isFatalConsoleMessage } = createRequire(import.meta.url)('../electron/runtime-errors.cjs');
const { inspectUnavailableDepth } = createRequire(import.meta.url)('../electron/depth-evidence.cjs');
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

test('renderer qualification rejects Bevy validation failures logged through console.info', () => {
    assert.equal(isFatalConsoleMessage('info', '%cERROR%c crates/bevy_render/error_handler.rs%c Caught rendering error: Validation Error'), true);
    assert.equal(isFatalConsoleMessage('info', '%cERROR%c %c Shader translation error for stage ShaderStages(FRAGMENT)'), true);
    assert.equal(isFatalConsoleMessage('error', 'Failed to load Bevy WASM'), true);
    assert.equal(isFatalConsoleMessage('info', 'panicked at renderer setup'), true);
});

test('renderer warnings do not stand in for actual render failure evidence', () => {
    assert.equal(isFatalConsoleMessage('warning', 'Automatic fallback to software WebGL has been deprecated.'), false);
    assert.equal(isFatalConsoleMessage('info', '%cWARN%c Software rendering is slow.'), false);
    assert.equal(isFatalConsoleMessage('info', 'Scene initialized with test objects'), false);
});

test('depth capability evidence requires the actual backend reason and accessible disabled state', () => {
    const reason = 'Depth view is unavailable with the current OpenGL/WebGL renderer';
    const state = { disabled: true, pressed: 'false', title: reason, status: reason,
        describedBy: 'depth-view-status', statusHidden: false };
    assert.equal(inspectUnavailableDepth(state), state);
    assert.throws(() => inspectUnavailableDepth({ ...state, disabled: false }), /GL depth control must be disabled/);
    assert.throws(() => inspectUnavailableDepth({ ...state, statusHidden: true }), /unavailability must be visible/);
});

test('still-waiting depth bootstrap cannot satisfy production capability qualification', () => {
    const reason = 'Checking depth view availability…';
    assert.throws(() => inspectUnavailableDepth({ disabled: true, pressed: 'false', title: reason,
        status: reason, describedBy: 'depth-view-status', statusHidden: false }), /resolve to the actual GL limitation/);
});

function selectionFixture() {
    const width = 128, height = 128;
    const bitmap = Buffer.alloc(width * height * 4);
    for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
        bitmap.set(x >= 30 && x < 70 && y >= 30 && y < 70
            ? [20, 30, 180, 255] : [80, 80, 80, 255], (y * width + x) * 4);
    }
    return { bitmap, width, height };
}

function addFixtureOutline(bitmap, width, { left = 30, top = 30, color = [0, 165, 255, 255], fill = false } = {}) {
    const after = Buffer.from(bitmap);
    for (let y = top; y < top + 40; y++) for (let x = left; x < left + 40; x++) {
        if (fill || x < left + 2 || x >= left + 38 || y < top + 2 || y >= top + 38) {
            after.set(color, (y * width + x) * 4);
        }
    }
    return after;
}

test('selection evidence locates a real red interior and accepts the inside-silhouette orange boundary', () => {
    const { bitmap, width, height } = selectionFixture();
    const object = locateRedObject(bitmap, width, height);
    assert.deepEqual(object.bounds, { left: 30, top: 30, right: 69, bottom: 69 });
    assert.ok(object.point.x > 35 && object.point.x < 65 && object.point.y > 35 && object.point.y < 65);
    const result = inspectSelectionOutline(bitmap, addFixtureOutline(bitmap, width), width, height);
    assert.equal(result.outlinePixels, 304);
    assert.equal(result.interiorOrangePixels, 0);
    assert.equal(result.outsideChangedPixels, 0);
});

test('selection geometry retains the shadowed red cube face without accepting neutral or other canonical materials', () => {
    const { bitmap, width, height } = selectionFixture();
    // The retained Electron frame has shadowed metallic cube pixels of exactly
    // RGB(71,32,49). A strict bright-red population mask fragments this face and
    // incorrectly excludes its valid orange silhouette from the boundary band.
    for (let y = 40; y < 70; y++) for (let x = 30; x < 70; x++) {
        bitmap.set([49, 32, 71, 255], (y * width + x) * 4);
    }
    const result = inspectSelectionOutline(bitmap, addFixtureOutline(bitmap, width), width, height);
    assert.equal(result.object.pixels, 1600);
    assert.equal(result.boundaryPixels, 156);
    assert.equal(result.outlinePixels, 304);
    assert.equal(result.interiorOrangePixels, 0);
    assert.equal(result.outsideChangedPixels, 0);
    for (const color of [[80, 80, 80, 255], [20, 180, 30, 255], [180, 30, 20, 255]]) {
        const other = Buffer.alloc(bitmap.length);
        for (let index = 0; index < other.length; index += 4) other.set(color, index);
        assert.throws(() => locateRedObject(other, width, height), /no selectable canonical red object/);
    }
});

test('selection evidence rejects unchanged and hover-only frames after the pointer leaves', () => {
    const { bitmap, width, height } = selectionFixture();
    // A hover outline that disappears when input parks off the object leaves this
    // same unchanged frame; only persistent selected-object pixels can qualify.
    assert.throws(() => inspectSelectionOutline(bitmap, Buffer.from(bitmap), width, height), /no visible orange outline/);
    assert.throws(() => locateRedObject(Buffer.alloc(bitmap.length), width, height), /no selectable canonical red object/);
});

test('selection evidence rejects wrong-color and wrong-object outlines', () => {
    const { bitmap, width, height } = selectionFixture();
    const wrongColor = addFixtureOutline(bitmap, width, { color: [255, 20, 20, 255] });
    const wrongObject = addFixtureOutline(bitmap, width, { left: 85, top: 85 });
    assert.throws(() => inspectSelectionOutline(bitmap, wrongColor, width, height), /no visible orange outline/);
    assert.throws(() => inspectSelectionOutline(bitmap, wrongObject, width, height), /no visible orange outline/);
});

test('selection evidence rejects orange fills instead of object boundaries', () => {
    const { bitmap, width, height } = selectionFixture();
    assert.throws(() => inspectSelectionOutline(bitmap, addFixtureOutline(bitmap, width, { fill: true }), width, height),
        /not localized|fill rather than an outline/);
});

test('selection evidence rejects a global tint even if a valid object outline is present', () => {
    const { bitmap, width, height } = selectionFixture();
    const after = addFixtureOutline(bitmap, width);
    for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
        if (x < 20 || x > 80 || y < 20 || y > 80) after[(y * width + x) * 4] += 40;
    }
    assert.throws(() => inspectSelectionOutline(bitmap, after, width, height), /changed the scene outside the selected object/);
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



test('qualification retains protected sandbox provisioning and safe software graphics', async () => {
    const { readFile } = await import('node:fs/promises');
    const read = name => readFile(new URL('../../' + name, import.meta.url), 'utf8');
    const workflow = await read('.github/workflows/electron-runtime.yml');
    const production = workflow.split('\n').find(line => line.includes('supervise.py production'));
    const environment = workflow.split('\n').find(line => line.includes('supervise.py environment'));
    for (const flag of ['--use-gl=angle', '--use-angle=swiftshader']) {
        assert.ok(production.includes(flag));
        assert.ok(!environment.includes(flag));
        assert.ok(!(await read('src-electron/main.ts')).includes(flag));
        assert.ok(!(await read('launcher.sh')).includes(flag));
    }
    for (const flag of ['--no-sandbox', '--disable-setuid-sandbox', '--disable-web-security', '--enable-unsafe-swiftshader']) {
        assert.ok(!workflow.includes(flag));
    }
    assert.match(workflow, /provision_ci_runtime\.py/);
    assert.ok(production.includes('$PENTIMENTO_CI_ELECTRON'));
    assert.ok(environment.includes('$PENTIMENTO_CI_ELECTRON'));
});
