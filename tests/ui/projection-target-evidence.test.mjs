import test from 'node:test';
import assert from 'node:assert/strict';
import { projectionReceipts, assertRenderedTarget, receiverHistoryRegion } from '../native/projection-target-evidence.mjs';

test('receipts describe actual atlas/image and independent source bounds', () => {
    const log = 'Projection target receipt: entity=7 name="Sphere" texels=500 atlas=aa image=bb geometry=cc bound=true undo=1 redo=0 active=false source_bounds=Some([500.0, 100.0, 600.0, 150.0]) target_bounds=Some([50.0, 100.0, 80.0, 130.0])';
    assert.deepEqual(projectionReceipts(log)[0].source_bounds, [500, 100, 600, 150]);
    assert.equal(projectionReceipts(log)[0].painted_texels, 500);
    assert.equal(projectionReceipts(log, 'Cube').length, 0);
});

test('source-only paint and unbound CPU targets cannot qualify receiver rendering', () => {
    const width = 800, height = 400;
    const before = { pixels: Buffer.alloc(width * height * 3) };
    const after = { pixels: Buffer.from(before.pixels) };
    for (let y = 100; y < 130; y++) for (let x = 50; x < 80; x++) { const i = (y * width + x) * 3; after.pixels[i] = 255; after.pixels[i + 2] = 255; }
    const r = { painted_texels: 500, texture_bound: true, target_bounds: [50, 100, 80, 130], source_bounds: [50, 100, 80, 130] };
    assert.throws(() => assertRenderedTarget(before, after, width, height, r), /independent rendered paint/);
    assert.throws(() => assertRenderedTarget(before, after, width, height, { ...r, texture_bound: false }), /bound paint/);
    assert.equal(assertRenderedTarget(before, after, width, height, { ...r, source_bounds: [500, 100, 600, 150] }).receiver_changed_magenta_pixels, 900);
});


test('history region rejects layered changes on the source alone', () => {
    const width = 800, height = 400;
    const baseline = { pixels: Buffer.alloc(width * height * 3) }, deformed = { pixels: Buffer.alloc(width * height * 3, 255) };
    const region = { width, height, baseline, deformed, pixels: Array.from({length: 150}, (_, i) => ({index: (110 * width + 510 + i % 30) * 3, budget: 2})) };
    const receipt = {target_bounds: [50,100,80,130], source_bounds: [500,100,600,150]};
    assert.throws(() => receiverHistoryRegion(region, [receipt, receipt]), /independent receiver change/);
});
