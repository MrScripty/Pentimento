// Native RGB readback only. Toolbar, side panels and parked cursor are excluded.
import assert from 'node:assert/strict';

function viewport(frame, width, height) {
    assert.equal(frame.pixels.length, width * height * 3);
    const region = { left: 30, top: 70, right: width - 335, bottom: height - 40 };
    assert.ok(region.right > region.left && region.bottom > region.top);
    return region;
}

export function assertNativeScene(frame, width, height) {
    const { left, top, right, bottom } = viewport(frame, width, height);
    const histogram = new Map(), tiles = new Array(12).fill(0);
    let painted = 0;
    for (let y = top; y < bottom; y++) for (let x = left; x < right; x++) {
        const index = (y * width + x) * 3;
        const [r, g, b] = frame.pixels.subarray(index, index + 3);
        const key = `${r >> 4},${g >> 4},${b >> 4}`;
        histogram.set(key, (histogram.get(key) ?? 0) + 1);
        if (Math.max(r, g, b) > 15) {
            painted++;
            const column = Math.floor((x - left) * 4 / (right - left));
            const row = Math.floor((y - top) * 3 / (bottom - top));
            tiles[row * 4 + column]++;
        }
    }
    // Native defaults intentionally include wireframe and atmospheric lighting;
    // do not impose the WASM material-palette oracle on that different output.
    const sampled = (right - left) * (bottom - top);
    const varied = 1 - Math.max(0, ...histogram.values()) / sampled;
    const occupiedTiles = tiles.filter(count => count > sampled * .0005).length;
    assert.ok(painted > sampled * .01 && occupiedTiles >= 4, 'Native scene has no distributed visible geometry');
    assert.ok(histogram.size >= 8 && varied > .01, 'Native scene is a blank or uniform clear surface');
    return { sampled_pixels: sampled, painted_pixels: painted, occupied_tiles: occupiedTiles,
        colors: histogram.size, varied_fraction: varied };
}

export function assertNativeDepthScene(before, after, width, height) {
    const { left, top, right, bottom } = viewport(after, width, height);
    viewport(before, width, height);
    const histogram = new Array(16).fill(0);
    let sampled = 0, changed = 0, chromatic = 0, gray = 0;
    for (let y = top; y < bottom; y++) for (let x = left; x < right; x++) {
        const index = (y * width + x) * 3;
        const [r, g, b] = after.pixels.subarray(index, index + 3);
        const high = Math.max(r, g, b), low = Math.min(r, g, b);
        if (high - low > 3) chromatic++;
        else if (high > 15) { gray++; histogram[Math.floor((r + g + b) / 48)]++; }
        if (Math.max(...[0, 1, 2].map(c => Math.abs(before.pixels[index + c] - after.pixels[index + c]))) > 20) changed++;
        sampled++;
    }
    const populatedBins = histogram.filter(count => count >= Math.max(1, sampled * .001)).length;
    assert.ok(gray > sampled * .05, 'Depth view has no broad visible grayscale geometry');
    assert.ok(populatedBins >= 4, 'Depth view lacks scene depth intensity variation');
    assert.ok(chromatic < sampled * .01, 'Depth view still contains the ordinary colored scene');
    assert.ok(changed > sampled * .05, 'Depth view has no meaningful presented scene transition');
    return { sampled_pixels: sampled, changed_pixels: changed, gray_pixels: gray,
        chromatic_pixels: chromatic, populated_intensity_bins: populatedBins };
}
