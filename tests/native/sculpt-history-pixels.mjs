// Native framebuffer history oracle. Compare the pixels that actually deformed,
// and calibrate each pixel against unchanged frames at both geometry endpoints.
import assert from 'node:assert/strict';

function delta(a, b, index) {
    return Math.max(...[0, 1, 2].map(channel => Math.abs(a.pixels[index + channel] - b.pixels[index + channel])));
}

function checkFrames(frames, width, height) {
    assert.ok(frames.length >= 2, 'History evidence needs unchanged-frame noise samples');
    for (const frame of frames) assert.equal(frame.pixels.length, width * height * 3);
}

export function deformationRegion(baselineFrames, deformedFrames, width, height) {
    checkFrames(baselineFrames, width, height);
    checkFrames(deformedFrames, width, height);
    const baseline = baselineFrames.at(-1), deformed = deformedFrames.at(-1);
    const pixels = [];
    let signal = 0, allowance = 0;
    for (let y = 70; y < height - 40; y++) for (let x = 30; x < width - 335; x++) {
        const index = (y * width + x) * 3;
        let noise = 0;
        for (const frames of [baselineFrames, deformedFrames]) {
            for (let i = 0; i < frames.length; i++) for (let j = i + 1; j < frames.length; j++) {
                noise = Math.max(noise, delta(frames[i], frames[j], index));
            }
        }
        // Two levels of quantization slack, on top of measured frame noise.
        const budget = noise + 2;
        const change = delta(baseline, deformed, index);
        if (change > Math.max(20, 4 * budget)) {
            pixels.push({ index, budget });
            signal += change;
            allowance += budget;
        }
    }
    assert.ok(pixels.length > 25, `No established visible deformation above unchanged-frame noise (${pixels.length} pixels)`);
    assert.ok(signal > 6 * allowance, 'Deformation is not distinguishable from unchanged-frame noise');
    return { pixels, baseline, deformed, width, height, signal, allowance };
}

export function assertRegionRestored(region, expected, beforeRestore, actual, label) {
    checkFrames([expected, beforeRestore, actual], region.width, region.height);
    let residual = 0, motion = 0, restored = 0, changed = 0;
    for (const { index, budget } of region.pixels) {
        const remaining = delta(expected, actual, index);
        const transition = delta(beforeRestore, actual, index);
        residual += remaining;
        motion += transition;
        if (remaining <= 2 * budget) restored++;
        if (transition > Math.max(20, 4 * budget)) changed++;
    }
    const count = region.pixels.length;
    // Both requirements matter: matching an endpoint alone cannot show that
    // undo/redo/cancel did anything, and motion alone cannot prove restoration.
    assert.ok(changed >= Math.ceil(count * 0.9), `${label}: no actual change across the deformation region (${changed}/${count})`);
    assert.ok(motion > 6 * region.allowance, `${label}: transition is indistinguishable from unchanged-frame noise`);
    assert.ok(restored >= Math.ceil(count * 0.95), `${label}: deformation pixels did not return to the expected endpoint (${restored}/${count})`);
    assert.ok(residual <= 2 * region.allowance, `${label}: regional residual exceeds calibrated unchanged-frame noise`);
    return {
        region_pixels: count,
        changed_region_pixels: changed,
        restored_region_pixels: restored,
        residual_mean: residual / count,
        measured_noise_budget_mean: region.allowance / count,
    };
}
