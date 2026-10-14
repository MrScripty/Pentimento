import test from 'node:test';
import assert from 'node:assert/strict';
import { assertNativeScene, assertNativeDepthScene } from '../native/depth-view-pixels.mjs';

const width = 480, height = 240;
function frame(color) {
    const pixels = new Uint8Array(width * height * 3);
    for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) pixels.set(color(x, y), (y * width + x) * 3);
    return { pixels };
}
const colorScene = frame((x, y) => {
    if (x < 60) return [100 + y % 100, 20, 20];
    if (x < 90) return [20, 100 + y % 100, 20];
    if (x < 120) return [20, 20, 100 + y % 100];
    return [80, 80, 80];
});
const depthScene = frame((x, y) => { const gray = 30 + (x + y) % 200; return [gray, gray, gray]; });

test('native baseline accepts wireframe-compatible variation but rejects blank, flat and toolbar-only frames', () => {
    assert.ok(assertNativeScene(colorScene, width, height).occupied_tiles >= 4);
    const wireframe = frame((x, y) => {
        const value = x % 10 === 0 || y % 10 === 0 ? 30 + (x + y) % 200 : 0;
        return [value, value, value];
    });
    assert.ok(assertNativeScene(wireframe, width, height).painted_pixels > 0);
    for (const invalid of [frame(() => [0, 0, 0]), frame(() => [80, 80, 80]),
        frame((_x, y) => y === 100 ? [200, 200, 200] : [0, 0, 0]),
        frame((_x, y) => y < 48 ? [150, 150, 150] : [0, 0, 0])]) {
        assert.throws(() => assertNativeScene(invalid, width, height));
    }
});

test('native depth requires a changed grayscale scene with broad depth variation', () => {
    const result = assertNativeDepthScene(colorScene, depthScene, width, height);
    assert.ok(result.changed_pixels > result.sampled_pixels * .05);
    assert.ok(result.populated_intensity_bins >= 4);
    for (const invalid of [frame(() => [0, 0, 0]), frame(() => [100, 100, 100]), colorScene]) {
        assert.throws(() => assertNativeDepthScene(colorScene, invalid, width, height));
    }
    assert.throws(() => assertNativeDepthScene(depthScene, depthScene, width, height), /no meaningful presented scene transition/);
});

test('native depth evidence excludes toolbar and side-panel changes', () => {
    const overlayOnly = { pixels: colorScene.pixels.slice() };
    for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
        if (y < 70 || x >= width - 335) overlayOnly.pixels.set([200, 200, 200], (y * width + x) * 3);
    }
    assert.throws(() => assertNativeDepthScene(colorScene, overlayOnly, width, height));
});
