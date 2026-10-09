// Actual rendered Svelte/bridge routing with a recorded mock IPC transport.
// Backend state is supplied as protocol receipts. Exact restoration/refusal is
// exercised separately by the production scene and input pipeline tests.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? 'playwright-core');
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH ?? '/usr/bin/chromium', headless: true, args: ['--no-sandbox'] });
try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(() => {
        window.commands = [];
        window.__PENTIMENTO_IPC__ = { postMessage: raw => window.commands.push(JSON.parse(raw)) };
    });
    await page.goto(process.env.PENTIMENTO_UI_URL ?? 'http://127.0.0.1:5187');
    await page.waitForFunction(() => window.commands.some(message => message.type === 'RequestBrushState'));
    const receive = message => page.evaluate(message => window.__PENTIMENTO_RECEIVE__(JSON.stringify(message)), message);
    const mode = value => receive({ type: 'EditModeChanged', data: { mode: value } });
    const paint = (undo, redo) => receive({ type: 'PaintBrushStateChanged', data: {
        settings: { preset_id: 0, customized: false, color: [1, 0, 1, 1], size: 14, opacity: 1, hardness: 0.7, spacing: 0.1, blend_mode: 'Normal' },
        presets: [], can_undo: undo, can_redo: redo, source_visible: false,
    } });
    const sculpt = (undo, redo, active = false) => receive({ type: 'SculptHistoryChanged', data: {
        undo_strokes: undo, redo_strokes: redo, active, notice: null,
    } });
    const historyCommands = () => page.evaluate(() => window.commands.filter(message =>
        ['PaintCommand', 'SculptCommand'].includes(message.type) && ['Undo', 'Redo'].includes(message.data)));
    const open = async () => {
        await page.getByRole('button', { name: 'Edit', exact: true }).click();
        return page.getByRole('menu', { name: 'Edit', exact: true });
    };
    const action = async (name, type, keyboard = false) => {
        const before = (await historyCommands()).length;
        const menu = await open();
        const button = menu.getByRole('menuitem', { name, exact: true });
        assert.equal(await button.isEnabled(), true);
        if (keyboard) { await button.focus(); await page.keyboard.press('Enter'); }
        else await button.click();
        await menu.waitFor({ state: 'hidden' });
        const emitted = (await historyCommands()).slice(before);
        assert.deepEqual(emitted, [{ type, data: name }], `${name}: exactly one active-tool command`);
    };
    // Positive routing fails on the former close-only menu implementation.
    await mode('Paint');
    const uninitialized = await open();
    assert.equal(await uninitialized.getByRole('menuitem', { name: 'Undo', exact: true }).isDisabled(), true);
    assert.equal(await uninitialized.getByRole('menuitem', { name: 'Redo', exact: true }).isDisabled(), true);
    await page.keyboard.press('Escape');
    await paint(true, false);
    await action('Undo', 'PaintCommand');
    await paint(false, true); await action('Redo', 'PaintCommand', true);
    await mode('Sculpt'); await sculpt(1, 0);
    await action('Undo', 'SculptCommand');
    await sculpt(0, 1); await action('Redo', 'SculptCommand', true);

    const disabled = async () => {
        const before = await historyCommands();
        const menu = await open();
        for (const name of ['Undo', 'Redo']) {
            const button = menu.getByRole('menuitem', { name, exact: true });
            assert.equal(await button.isDisabled(), true, `${name}: unavailable history must be disabled`);
            // Click an actual disabled button's coordinates; no forced DOM event.
            const box = await button.boundingBox();
            await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
        }
        assert.deepEqual(await historyCommands(), before);
        await page.keyboard.press('Escape');
        await menu.waitFor({ state: 'hidden' });
    };
    await sculpt(1, 1, true); await disabled();
    await sculpt(0, 0); await disabled();
    await mode('Paint'); await paint(false, false); await disabled();
    // Cached history belonging to another mode cannot enable unsupported modes.
    await paint(true, true); await sculpt(1, 1);
    for (const value of ['None', 'MeshEdit']) { await mode(value); await disabled(); }
    // Availability changing while a menu is already open is authoritative.
    await mode('Sculpt'); await sculpt(1, 1);
    const menu = await open(); await sculpt(1, 1, true);
    assert.equal(await menu.getByRole('menuitem', { name: 'Undo', exact: true }).isDisabled(), true);
    assert.equal(await menu.getByRole('menuitem', { name: 'Redo', exact: true }).isDisabled(), true);
    await page.keyboard.press('Escape');
    await mode('Paint'); await paint(false, true);
    await action('Redo', 'PaintCommand');
    assert.deepEqual(errors, []);
    const output = process.env.PENTIMENTO_EVIDENCE_DIR ?? '/tmp/pentimento-history-menu';
    await mkdir(output, { recursive: true });
    await open();
    await page.screenshot({ path: `${output}/edit-history-menu.jpg`, type: 'jpeg', quality: 85 });
    console.log(JSON.stringify({ type: 'pentimento.ui.history_menu', status: 'passed', commands: await historyCommands(), screenshot: 'edit-history-menu.jpg', renderer_qualification: false }));
} finally {
    await browser.close();
}
