import assert from 'node:assert/strict';

export function projectionReceipts(log, name = 'Sphere') {
    const pattern = /Projection target receipt: entity=(\d+) name=("[^"\n]*") texels=(\d+) atlas=([a-f0-9]+) image=([a-f0-9]+) geometry=([a-f0-9]+) bound=(true|false) undo=(\d+) redo=(\d+) active=(true|false) source_bounds=(None|Some\(\[[^\]]*\]\)) target_bounds=(None|Some\(\[[^\]]*\]\))/g;
    const bounds = text => text === 'None' ? null : JSON.parse(text.slice(5, -1));
    return [...log.matchAll(pattern)].filter(match => JSON.parse(match[2]) === name).map(match => ({
        entity: match[1], name, painted_texels: Number(match[3]), atlas: match[4], image: match[5], geometry: match[6],
        texture_bound: match[7] === 'true', undo: Number(match[8]), redo: Number(match[9]), active: match[10] === 'true',
        source_bounds: bounds(match[11]), target_bounds: bounds(match[12]),
    }));
}

// Only count changed magenta pixels on the actual receiver outside the source's
// projected painted bounds. A visible source canvas cannot satisfy this oracle.
export function assertRenderedTarget(before, after, width, height, receipt) {
    assert.ok(receipt.texture_bound && receipt.painted_texels > 100, 'Actual receiver atlas/image must contain bound paint');
    assert.ok(receipt.target_bounds, 'Receiver must be visible in the inspection camera');
    const [x0, y0, x1, y1] = receipt.target_bounds;
    let changed = 0, sampled = 0;
    for (let y = Math.max(70, Math.floor(y0)); y < Math.min(height - 40, Math.ceil(y1)); y++) {
        for (let x = Math.max(30, Math.floor(x0)); x < Math.min(width - 335, Math.ceil(x1)); x++) {
            const source = receipt.source_bounds;
            if (source && x >= source[0] - 3 && x <= source[2] + 3 && y >= source[1] - 3 && y <= source[3] + 3) continue;
            sampled++;
            const i = (y * width + x) * 3;
            const [r, g, b] = after.pixels.subarray(i, i + 3);
            if (r > g + 25 && b > g + 25 && Math.max(...[0, 1, 2].map(c => Math.abs(after.pixels[i + c] - before.pixels[i + c]))) > 20) changed++;
        }
    }
    assert.ok(changed > 100, `Target surface lacks independent rendered paint: ${changed}/${sampled} magenta pixels outside source bounds`);
    return { receiver_changed_magenta_pixels: changed, receiver_sampled_pixels: sampled };
}

export function receiverPixel(x, y, receipts) {
    return receipts.every(({ target_bounds: target, source_bounds: source }) => target && x >= target[0] && x <= target[2] && y >= target[1] && y <= target[3]
        && !(source && x >= source[0] - 3 && x <= source[2] + 3 && y >= source[1] - 3 && y <= source[3] + 3));
}

export function receiverHistoryRegion(region, receipts) {
    const pixels = region.pixels.filter(({ index }) => {
        const xy = index / 3;
        return receiverPixel(xy % region.width, Math.floor(xy / region.width), receipts);
    });
    const signal = pixels.reduce((sum, { index }) => sum + Math.max(...[0, 1, 2].map(c => Math.abs(region.baseline.pixels[index + c] - region.deformed.pixels[index + c]))), 0);
    const allowance = pixels.reduce((sum, { budget }) => sum + budget, 0);
    assert.ok(pixels.length > 100 && signal > 6 * allowance, `Layered projection lacks independent receiver change (${pixels.length} pixels)`);
    return { ...region, pixels, signal, allowance };
}

export function assertReceiverEndpoint(region, expected, actual, receipt) {
    let restored = 0, residual = 0;
    for (const { index, budget } of region.pixels) {
        const xy = index / 3;
        assert.ok(receiverPixel(xy % region.width, Math.floor(xy / region.width), [receipt]), 'Source covers receiver history evidence');
        const difference = Math.max(...[0, 1, 2].map(c => Math.abs(expected.pixels[index + c] - actual.pixels[index + c])));
        residual += difference;
        if (difference <= 2 * budget) restored++;
    }
    assert.ok(restored >= Math.ceil(region.pixels.length * .95) && residual <= 2 * region.allowance, 'Rendered receiver endpoint differs from committed pixels');
    return { receiver_restored_pixels: restored, receiver_region_pixels: region.pixels.length, receiver_residual_mean: residual / region.pixels.length };
}
