import test from 'node:test';
import assert from 'node:assert/strict';
import { clampMenuPosition } from '../../ui/src/lib/menuPosition.ts';

test('menu preserves an interior anchor and clamps bottom/right edges', () => {
    const size = { width: 150, height: 334 };
    const viewport = { width: 1920, height: 1080 };
    assert.deepEqual(clampMenuPosition({ x: 300, y: 200 }, size, viewport), { x: 300, y: 200 });
    assert.deepEqual(clampMenuPosition({ x: 35, y: 1045 }, size, viewport), { x: 35, y: 738 });
    assert.deepEqual(clampMenuPosition({ x: 1919, y: 1079 }, size, viewport), { x: 1762, y: 738 });
    assert.deepEqual(clampMenuPosition({ x: -10, y: -20 }, size, viewport), { x: 8, y: 8 });
});

test('menu uses its CSS-constrained measured size in small windows and on resize', () => {
    for (const viewport of [{ width: 180, height: 180 }, { width: 320, height: 240 }]) {
        const menu = { width: Math.min(150, viewport.width - 16), height: viewport.height - 16 };
        const placed = clampMenuPosition({ x: 1919, y: 1079 }, menu, viewport);
        assert.ok(placed.x >= 8 && placed.y >= 8);
        assert.ok(placed.x + menu.width <= viewport.width - 8);
        assert.ok(placed.y + menu.height <= viewport.height - 8);
    }
});
