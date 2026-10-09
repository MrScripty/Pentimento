import test from 'node:test';
import assert from 'node:assert/strict';
import { colorToHex, hexToColor } from '../../ui/src/lib/brush-values.ts';

test('color picker preserves the displayed sRGB color through linear painting state', () => {
    for (const hex of ['#000000', '#ffffff', '#808080', '#f05a24', '#4080c0']) {
        assert.equal(colorToHex(hexToColor(hex)), hex);
    }
    assert.ok(Math.abs(hexToColor('#808080')[0] - 0.21586) < 0.00001);
});
