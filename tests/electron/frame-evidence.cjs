const assert = require('node:assert/strict');
// NativeImage's Linux bitmap is BGRA. Expect the canonical ScenePlugin's red cube,
// green torus, blue sphere and neutral ground; a blank/gradient-only canvas is insufficient.
function inspectScene(bitmap, width, height) {
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
    assert.ok(histogram.size >= 8 && variedFraction > .01, 'canvas lacks spatial scene detail');
    for (const color of ['red', 'green', 'blue']) {
        assert.ok(counts[color] >= Math.max(32, total * .0001), `missing canonical ${color} object pixels`);
    }
    assert.ok(counts.neutral > total * .01, 'missing neutral ground/background pixels');
    return { colors: histogram.size, variedFraction, counts };
}

function redObject(bitmap, width, height) {
    assert.equal(bitmap.length, width * height * 4);
    const red = new Uint8Array(width * height);
    for (let pixel = 0; pixel < red.length; pixel++) {
        const [b, g, r, a] = bitmap.subarray(pixel * 4, pixel * 4 + 4);
        // Geometry needs the canonical red cube's shadowed faces as well as its
        // bright material. Real metallic-face pixels such as RGB(71,32,49) are
        // still red-dominant but fail the stricter scene-population blue ratio.
        // Keep strong red/green separation; neutral, blue and green remain out.
        // inspectScene's original material-population predicate is unchanged.
        red[pixel] = a >= 240 && r > g * 1.3 + 8 && r > b * 1.1 + 8 ? 1 : 0;
    }
    const queue = new Int32Array(red.length);
    let largest = [];
    for (let seed = 0; seed < red.length; seed++) {
        if (red[seed] !== 1) continue;
        let count = 1;
        queue[0] = seed;
        red[seed] = 2;
        for (let cursor = 0; cursor < count; cursor++) {
            const index = queue[cursor], x = index % width, y = Math.floor(index / width);
            for (const neighbor of [x > 0 ? index - 1 : -1, x + 1 < width ? index + 1 : -1,
                y > 0 ? index - width : -1, y + 1 < height ? index + width : -1]) {
                if (neighbor >= 0 && red[neighbor] === 1) {
                    red[neighbor] = 2;
                    queue[count++] = neighbor;
                }
            }
        }
        if (count > largest.length) largest = Array.from(queue.subarray(0, count));
    }
    assert.ok(largest.length >= 32, 'no selectable canonical red object in the actual scene');
    const mask = new Uint8Array(red.length);
    let sumX = 0, sumY = 0;
    const bounds = { left: width, top: height, right: 0, bottom: 0 };
    for (const index of largest) {
        mask[index] = 1;
        const x = index % width, y = Math.floor(index / width);
        sumX += x; sumY += y;
        bounds.left = Math.min(bounds.left, x); bounds.right = Math.max(bounds.right, x);
        bounds.top = Math.min(bounds.top, y); bounds.bottom = Math.max(bounds.bottom, y);
    }
    const cx = sumX / largest.length, cy = sumY / largest.length;
    let point, distance = Infinity;
    const boundary = [];
    for (const index of largest) {
        const x = index % width, y = Math.floor(index / width);
        const edge = x === 0 || y === 0 || x + 1 === width || y + 1 === height
            || !mask[index - 1] || !mask[index + 1] || !mask[index - width] || !mask[index + width];
        if (edge) boundary.push(index);
        const d = (x - cx) ** 2 + (y - cy) ** 2;
        if (!edge && d < distance) { point = { x, y }; distance = d; }
    }
    assert.ok(point, 'canonical red object has no interior click target');
    return { mask, boundary, point, bounds, pixels: largest.length };
}

function locateRedObject(bitmap, width, height) {
    const { point, bounds, pixels } = redObject(bitmap, width, height);
    return { point, bounds, pixels };
}

function inspectSelectionOutline(before, after, width, height) {
    assert.equal(after.length, before.length);
    const object = redObject(before, width, height);
    const nearBoundary = new Uint8Array(width * height);
    // The production outline is two screen pixels thick; allow its antialiased
    // edge to straddle the observed material boundary without accepting a fill.
    for (const index of object.boundary) {
        const x = index % width, y = Math.floor(index / width);
        for (let dy = -4; dy <= 4; dy++) for (let dx = -4; dx <= 4; dx++) {
            const nx = x + dx, ny = y + dy;
            if (nx >= 0 && nx < width && ny >= 0 && ny < height) nearBoundary[ny * width + nx] = 1;
        }
    }
    let newOrange = 0, outline = 0, interiorOrange = 0, outsideChanged = 0, outsidePixels = 0;
    for (let pixel = 0; pixel < width * height; pixel++) {
        const offset = pixel * 4;
        const [b, g, r, a] = after.subarray(offset, offset + 4);
        const changed = Math.max(Math.abs(after[offset] - before[offset]),
            Math.abs(after[offset + 1] - before[offset + 1]), Math.abs(after[offset + 2] - before[offset + 2])) > 25;
        const x = pixel % width, y = Math.floor(pixel / width);
        if (x < object.bounds.left - 12 || x > object.bounds.right + 12
            || y < object.bounds.top - 12 || y > object.bounds.bottom + 12) {
            outsidePixels++;
            if (changed) outsideChanged++;
        }
        // Canonical ORANGE outline; a red material or wrong-color border fails.
        if (!changed || a < 240 || r < 180 || g < 70 || g > 215 || b > 90 || r < g + 25) continue;
        newOrange++;
        if (nearBoundary[pixel]) outline++;
        if (object.mask[pixel] && !nearBoundary[pixel]) interiorOrange++;
    }
    assert.ok(outline >= Math.max(16, object.boundary.length * .15), 'selection produced no visible orange outline at the clicked object boundary');
    assert.ok(outline >= newOrange * .8, 'orange changes are not localized to the clicked object boundary');
    assert.ok(interiorOrange <= Math.max(4, object.pixels * .01), 'selection evidence is an orange fill rather than an outline');
    assert.ok(outsideChanged <= outsidePixels * .005, 'selection changed the scene outside the selected object');
    return { object: { point: object.point, bounds: object.bounds, pixels: object.pixels },
        boundaryPixels: object.boundary.length, outlinePixels: outline, newOrangePixels: newOrange,
        interiorOrangePixels: interiorOrange, outsideChangedPixels: outsideChanged, outsidePixels };
}

module.exports = { inspectScene, locateRedObject, inspectSelectionOutline };
