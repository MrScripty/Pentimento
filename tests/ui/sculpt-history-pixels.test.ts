import test from 'node:test';
import assert from 'node:assert/strict';
import { deformationRegion, assertRegionRestored } from '../native/sculpt-history-pixels.mjs';

const width = 1280, height = 720;
const frame = () => ({ pixels: new Uint8Array(width * height * 3) });
function mark(source, value, count = 100, start = 100) {
    const output = { pixels: source.pixels.slice() };
    for (let x = start; x < start + count; x++) for (let c = 0; c < 3; c++) output.pixels[(100 * width + x) * 3 + c] = value;
    return output;
}
const baseline = frame(), deformed = mark(baseline, 255);
const stable = value => [value, value, value];
const region = () => deformationRegion(stable(baseline), stable(deformed), width, height);

test('history oracle rejects unchanged 100-pixel deformation for undo', () => {
    assert.throws(() => assertRegionRestored(region(), baseline, deformed, deformed, 'Undo'), /no actual change|did not return|residual/);
});

test('history oracle rejects a no-op redo and cancel', () => {
    assert.throws(() => assertRegionRestored(region(), deformed, baseline, baseline, 'Redo'), /no actual change|did not return|residual/);
    assert.throws(() => assertRegionRestored(region(), baseline, deformed, deformed, 'Cancel'), /no actual change|did not return|residual/);
});

test('history oracle requires visible pre-cancel deformation', () => {
    assert.throws(() => deformationRegion(stable(baseline), stable(baseline), width, height), /No established visible deformation/);
});

test('history oracle accepts actual restoration in both directions', () => {
    const area = region();
    assert.equal(assertRegionRestored(area, baseline, deformed, baseline, 'Undo').restored_region_pixels, 100);
    assert.equal(assertRegionRestored(area, deformed, baseline, deformed, 'Redo').changed_region_pixels, 100);
});

test('history oracle rejects partial restoration even with large unrelated motion', () => {
    const partial = mark(baseline, 255, 50);
    const unrelated = mark(partial, 255, 300, 500);
    assert.throws(() => assertRegionRestored(region(), baseline, deformed, unrelated, 'Undo'), /no actual change|did not return|residual/);
});

test('history oracle calibrates unchanged-frame noise and accepts within-noise restoration', () => {
    const baseFrames = [baseline, mark(baseline, 1), baseline];
    const changedFrames = [deformed, mark(baseline, 254), deformed];
    const area = deformationRegion(baseFrames, changedFrames, width, height);
    assert.equal(assertRegionRestored(area, baseline, deformed, mark(baseline, 2), 'Undo').restored_region_pixels, 100);
});

test('history oracle refuses unstable frames as evidence of deformation', () => {
    assert.throws(() => deformationRegion([baseline, deformed, baseline], stable(deformed), width, height), /No established visible deformation/);
});

test('history oracle rejects leftover geometry beyond measured regional noise', () => {
    assert.throws(() => assertRegionRestored(region(), baseline, deformed, mark(baseline, 10), 'Undo'), /did not return|residual/);
});
