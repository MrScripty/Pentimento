import assert from 'node:assert/strict';

// Native RGB pixels along the actual X11 drag path. A single endpoint dot (or
// two isolated endpoint dots) cannot satisfy all four-pixel cross-sections.
export function assertContinuousPaint(before, after, width, height, start, end) {
    assert.equal(before.pixels.length, width * height * 3);
    assert.equal(after.pixels.length, width * height * 3);
    const length = Math.hypot(end.x - start.x, end.y - start.y);
    assert.ok(length >= 50, 'Continuous paint evidence needs an extended path');
    const intervals = Math.ceil(length / 4);
    let covered = 0;
    for (let i = 0; i <= intervals; i++) {
        const x = Math.round(start.x + (end.x - start.x) * i / intervals);
        const y = Math.round(start.y + (end.y - start.y) * i / intervals);
        let colored = 0;
        for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) {
            if (dx * dx + dy * dy > 4) continue;
            const px = x + dx, py = y + dy;
            if (px < 0 || px >= width || py < 0 || py >= height) continue;
            const offset = (py * width + px) * 3;
            const [r, g, b] = after.pixels.subarray(offset, offset + 3);
            const changed = Math.max(...[0,1,2].map(c => Math.abs(after.pixels[offset+c] - before.pixels[offset+c]))) > 20;
            if (changed && r > g + 25 && b > g + 25) colored++;
        }
        assert.ok(colored >= 3, `Paint path is not continuously visible at sample ${i}/${intervals} (${x},${y}): ${colored} changed magenta pixels`);
        covered++;
    }
    return { path_samples: intervals + 1, covered_samples: covered, path_length_pixels: length };
}
