// Rendered Svelte/IPC contract check. Engine deformation/pixels are covered in
// crates/scene/src/{brush_ui,sculpt_mode}.rs; this harness does not fake a renderer.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? 'playwright-core');
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH ?? '/usr/bin/chromium', headless: true, args: ['--no-sandbox'] });
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
const errors = [];
page.on('pageerror', error => errors.push(error.message));
await page.addInitScript(() => {
    window.commands = [];
    window.__PENTIMENTO_IPC__ = { postMessage: message => window.commands.push(JSON.parse(message)) };
});
await page.goto(process.env.PENTIMENTO_UI_URL ?? 'http://127.0.0.1:5187');
await page.waitForFunction(() => window.commands.some(message => message.type === 'RequestBrushState'));
const receive = message => page.evaluate(message => window.__PENTIMENTO_RECEIVE__(JSON.stringify(message)), message);
const lastCommand = type => page.evaluate(type => window.commands.filter(message => message.type === type).at(-1), type);
// A rejected mesh never enters Sculpt, so its error must be visible before any
// brush panel exists, including when mesh-edit mode was previously active.
for (const mode of ['None', 'MeshEdit']) {
    await receive({ type: 'EditModeChanged', data: { mode } });
    await receive({ type: 'Error', data: { code: 'sculpt_target_unavailable', message: 'Cannot sculpt this mesh: invalid topology' } });
    assert.equal(await page.getByRole('alert').textContent().then(text => text.includes('Cannot sculpt this mesh')), true);
    await page.waitForTimeout(80);
    assert.ok((await lastCommand('LayoutUpdate')).data.regions.some(region => region.id === 'global-error'));
    await page.getByRole('button', { name: 'Dismiss error' }).click();
    assert.equal(await page.getByRole('alert').count(), 0);
}

let paint = { preset_id: 0, customized: true, color: [0.2158605, 0.2158605, 0.2158605, 1], size: 50, opacity: 1, hardness: 0.8, spacing: 0.25, blend_mode: 'Normal' };
const paintState = () => receive({ type: 'PaintBrushStateChanged', data: { settings: paint, presets: [{ id: 0, name: 'Hard Round' }, { id: 1, name: 'Soft Round' }], can_undo: true } });
await paintState();
await receive({ type: 'EditModeChanged', data: { mode: 'Paint' } });
await page.getByRole('heading', { name: 'Projection paint' }).waitFor();
await page.getByRole('button', { name: 'Eraser', exact: true }).click();
assert.deepEqual((await lastCommand('PaintCommand')).data, { SetBlendMode: { mode: 'Erase' } });
paint.blend_mode = 'Erase'; await paintState();
assert.equal(await page.locator('#paint-color').isDisabled(), true);
await page.getByRole('button', { name: 'Brush', exact: true }).click();
paint.blend_mode = 'Normal'; await paintState();
for (const [label, value, expected] of [
    ['Radius value', '17.5', { SetBrushSize: { size: 35 } }],
    ['Opacity value', '37', { SetBrushOpacity: { opacity: 0.37 } }],
    ['Hardness / falloff value', '15', { SetBrushHardness: { hardness: 0.15 } }],
    ['Dab spacing value', '40', { SetBrushSpacing: { spacing: 0.4 } }],
]) {
    await page.getByRole('spinbutton', { name: label, exact: true }).fill(value);
    await page.getByRole('spinbutton', { name: label, exact: true }).press('Enter');
    assert.deepEqual((await lastCommand('PaintCommand')).data, expected);
}
await page.getByLabel('Hex color', { exact: true }).fill('#4080c0');
await page.getByLabel('Hex color', { exact: true }).press('Enter');
const color = (await lastCommand('PaintCommand')).data.SetBrushColor.color;
assert.ok(Math.abs(color[0] - 0.05126946) < 0.000001);
await page.getByLabel('Brush preset', { exact: true }).selectOption('1');
assert.deepEqual((await lastCommand('PaintCommand')).data, { SelectBrushPreset: { preset_id: 1 } });
await page.getByRole('button', { name: 'Undo canvas stroke' }).click();
assert.equal((await lastCommand('PaintCommand')).data, 'Undo');
await page.getByLabel('Live projection', { exact: true }).check();
assert.deepEqual((await lastCommand('PaintCommand')).data, { SetLiveProjection: { enabled: true } });
await page.getByRole('button', { name: 'Apply canvas to UV surfaces' }).click();
assert.equal((await lastCommand('PaintCommand')).data, 'ProjectToScene');
const output = process.env.PENTIMENTO_EVIDENCE_DIR ?? '/tmp/pentimento-brush-evidence';
await mkdir(output, { recursive: true });
await page.screenshot({ path: `${output}/projection-paint-controls.jpg`, type: 'jpeg', quality: 85 });
let sculpt = { tool: 'Push', radius: 0.5, strength: 1, hardness: 0.5, falloff: 'Smooth' };
await receive({ type: 'SculptBrushStateChanged', data: { settings: sculpt } });
await receive({ type: 'EditModeChanged', data: { mode: 'Sculpt' } });
await page.getByRole('heading', { name: 'Sculpt brushes' }).waitFor();
assert.equal(await page.getByRole('heading', { name: 'Projection paint' }).count(), 0);
for (const tool of ['Push', 'Pull', 'Grab', 'Smooth', 'Flatten', 'Inflate', 'Pinch', 'Crease']) {
    await page.getByRole('button', { name: tool, exact: true }).click();
    assert.deepEqual((await lastCommand('SculptCommand')).data, { SetTool: { tool } });
}
for (const [label, value, expected] of [
    ['Radius value', '1.25', { SetRadius: { radius: 1.25 } }],
    ['Strength value', '42', { SetStrength: { strength: 0.42 } }],
    ['Hardness value', '25', { SetHardness: { hardness: 0.25 } }],
]) {
    await page.getByRole('spinbutton', { name: label, exact: true }).fill(value);
    await page.getByRole('spinbutton', { name: label, exact: true }).press('Enter');
    assert.deepEqual((await lastCommand('SculptCommand')).data, expected);
}
await page.getByLabel('Falloff curve').selectOption('Sharp');
assert.deepEqual((await lastCommand('SculptCommand')).data, { SetFalloff: { falloff: 'Sharp' } });
// A keyboard-originated backend update replaces displayed values, including tool.
sculpt = { tool: 'Grab', radius: 1.7, strength: 0.4, hardness: 0.25, falloff: 'Sharp' };
await receive({ type: 'SculptBrushStateChanged', data: { settings: sculpt } });
assert.equal(await page.getByRole('spinbutton', { name: 'Radius value', exact: true }).inputValue(), '1.7');
await page.screenshot({ path: `${output}/sculpt-brush-controls.jpg`, type: 'jpeg', quality: 85 });
// History availability comes from the native owner; buttons emit real protocol commands.
const sculptUndo = page.getByRole('button', { name: 'Undo sculpt stroke', exact: true });
const sculptRedo = page.getByRole('button', { name: 'Redo sculpt stroke', exact: true });
assert.equal(await sculptUndo.isDisabled(), true);
assert.equal(await sculptRedo.isDisabled(), true);
await receive({ type: 'SculptHistoryChanged', data: { undo_strokes: 1, redo_strokes: 0, active: false, notice: null } });
await sculptUndo.click();
assert.equal((await lastCommand('SculptCommand')).data, 'Undo');
await receive({ type: 'SculptHistoryChanged', data: { undo_strokes: 0, redo_strokes: 1, active: false, notice: null } });
await sculptRedo.click();
assert.equal((await lastCommand('SculptCommand')).data, 'Redo');
await receive({ type: 'SculptHistoryChanged', data: { undo_strokes: 1, redo_strokes: 1, active: true, notice: null } });
assert.equal(await sculptUndo.isDisabled(), true);
assert.equal(await sculptRedo.isDisabled(), true);
await receive({ type: 'SculptHistoryChanged', data: { undo_strokes: 0, redo_strokes: 0, active: false, notice: 'This stroke exceeds the local history limit and cannot be undone.' } });
assert.match(await page.getByRole('status').textContent(), /cannot be undone/);
// Mode unmount/remount keeps each backend-owned brush independently.
for (const mode of ['None', 'Paint', 'Sculpt', 'Paint']) await receive({ type: 'EditModeChanged', data: { mode } });
assert.equal(await page.getByRole('spinbutton', { name: 'Radius value', exact: true }).inputValue(), '25');
await page.waitForTimeout(80);
const layout = (await lastCommand('LayoutUpdate')).data;
const panel = layout.regions.find(region => region.id === 'brush-panel');
assert.ok(panel && panel.x > 900 && panel.width === 300 && panel.height > 500);
await page.getByRole('spinbutton', { name: 'Radius value', exact: true }).focus();
await page.waitForTimeout(10);
assert.equal((await lastCommand('SetUiInputCapture')).data.keyboard, true);
await page.locator('body').click({ position: { x: 500, y: 500 } });
await page.waitForTimeout(10);
assert.equal((await lastCommand('SetUiInputCapture')).data.keyboard, false);
await page.setViewportSize({ width: 560, height: 620 });
await page.waitForTimeout(80);
const mobilePanel = await page.locator('.brush-panel').boundingBox();
assert.ok(mobilePanel.x >= 0 && mobilePanel.x + mobilePanel.width <= 560);
// The native gate opens Add Object near the bottom edge. Every action must stay
// reachable inside the viewport; a native pointer must never be clamped to Cube.
await receive({ type: 'EditModeChanged', data: { mode: 'None' } });
for (const [w, h, x, y] of [[1280, 900, 35, 865], [1280, 900, 1279, 899], [180, 180, 172, 172]]) {
    await page.setViewportSize({ width: w, height: h });
    await page.mouse.move(x, y);
    await page.keyboard.press('Shift+A');
    const menu = page.getByRole('dialog', { name: 'Add Object', exact: true });
    await menu.waitFor();
    await page.waitForFunction(() => {
        const rect = document.querySelector('.add-menu')?.getBoundingClientRect();
        return rect && rect.x >= 0 && rect.y >= 0 && rect.right <= innerWidth && rect.bottom <= innerHeight;
    });
    if (h === 180) assert.equal(await menu.evaluate(element => element.scrollHeight > element.clientHeight), true);
    const addPaint = menu.getByRole('button', { name: 'Paint', exact: true });
    await addPaint.scrollIntoViewIfNeeded();
    const paintBounds = await addPaint.boundingBox();
    assert.ok(paintBounds.x >= 0 && paintBounds.y >= 0 && paintBounds.x + paintBounds.width <= w && paintBounds.y + paintBounds.height <= h);
    const menuScreenshot = `add-menu-${w}x${h}-${x}-${y}.jpg`;
    await page.screenshot({ path: `${output}/${menuScreenshot}`, type: 'jpeg', quality: 85 });
    const beforeCommands = await page.evaluate(() => window.commands.length);
    await addPaint.click();
    await menu.waitFor({ state: 'hidden' });
    const emitted = await page.evaluate(offset => window.commands.slice(offset), beforeCommands);
    assert.equal(emitted.filter(message => message.type === 'AddPaintCanvas').length, 1);
    assert.equal(emitted.filter(message => message.type === 'AddObject').length, 0);
    console.log(JSON.stringify({ type: 'pentimento.ui.menu', status: 'passed', viewport: { width: w, height: h }, anchor: { x, y }, paint_bounds: paintBounds, screenshot: menuScreenshot }));
}
// Resizing an already-open menu remeasures its overflow-constrained dimensions.
await page.setViewportSize({ width: 1280, height: 900 });
await page.mouse.move(1279, 899); await page.keyboard.press('Shift+A');
await page.getByRole('dialog', { name: 'Add Object', exact: true }).waitFor();
await page.setViewportSize({ width: 240, height: 180 });
await page.waitForFunction(() => {
    const rect = document.querySelector('.add-menu')?.getBoundingClientRect();
    return rect && rect.x >= 0 && rect.y >= 0 && rect.right <= innerWidth && rect.bottom <= innerHeight;
});
await page.keyboard.press('Escape');
await page.getByRole('dialog', { name: 'Add Object', exact: true }).waitFor({ state: 'hidden' });
assert.deepEqual(errors, []);
await browser.close();
console.log('Rendered controls passed: paint/erase, all numeric settings, color, presets, undo, live/apply, all 8 sculpt tools, falloff, backend sync, mode switches, layout/focus narrow viewport and edge/small-window Add Object menus.');
