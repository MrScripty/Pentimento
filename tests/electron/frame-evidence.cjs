const assert = require('node:assert/strict');
// NativeImage's Linux bitmap is BGRA. Expect the canonical ScenePlugin's red cube,
// green torus, blue sphere and neutral ground; a blank/gradient-only canvas is insufficient.
function measureScene(bitmap, width, height) {
    assert.equal(bitmap.length, width * height * 4);
    const counts = { red: 0, green: 0, blue: 0, neutral: 0 };
    const histogram = new Map();
    for (let index = 0; index < bitmap.length; index += 4) {
        const [b, g, r, a] = bitmap.subarray(index, index + 4);
        if (a < 240) continue;
        const key = `${b >> 4},${g >> 4},${r >> 4}`;
        histogram.set(key, (histogram.get(key) || 0) + 1);
        if (r > g * 1.3 + 8 && r > b * 1.3 + 8) counts.red++;
        if (g > r * 1.3 + 8 && g > b * 1.3 + 8) counts.green++;
        if (b > r * 1.3 + 8 && b > g * 1.05 + 3) counts.blue++;
        if (Math.max(r, g, b) - Math.min(r, g, b) < 12 && r > 15 && r < 240) counts.neutral++;
    }
    const total = width * height;
    const variedFraction = 1 - Math.max(0, ...histogram.values()) / total;
    const pending = [];
    if (!(histogram.size >= 8 && variedFraction > .01)) pending.push('canvas lacks spatial scene detail');
    for (const color of ['red', 'green', 'blue']) {
        if (!(counts[color] >= Math.max(32, total * .0001))) pending.push(`missing canonical ${color} object pixels`);
    }
    if (!(counts.neutral > total * .01)) pending.push('missing neutral ground/background pixels');
    return { colors: histogram.size, variedFraction, counts, ready: pending.length === 0, pending };
}
function inspectScene(bitmap, width, height) {
    const evidence = measureScene(bitmap, width, height);
    assert.ok(evidence.ready, evidence.pending.join('; '));
    return evidence;
}
module.exports = { inspectScene, measureScene };
