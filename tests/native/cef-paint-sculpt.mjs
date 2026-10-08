// Actual CEF + Bevy app qualification. DOM queries use the real CEF renderer;
// every user action travels through X11, Bevy input forwarding and production IPC.
// This script never installs a mock bridge or sends an engine command directly.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, openSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { chromium } from 'playwright-core';
import { deformationRegion, assertRegionRestored } from './sculpt-history-pixels.mjs';
import { assertContinuousPaint } from './paint-stroke-pixels.mjs';
import { cefFramebufferReceipt, startedStroke, completedStroke, assertAcceptedStroke, waitForSculptPresentation, parkedPointerFrames, holdNativeKey, tapNativeShortcut, assertNativeClickBounds, assertPaintedUiRegion, cefRenderingArguments } from './readiness.mjs';

const root = resolve(import.meta.dirname, '../..');
const cefRendering = process.env.PENTIMENTO_CEF_RENDERING ?? 'default';
const cefRenderingArgs = cefRenderingArguments(cefRendering);
const out = process.env.PENTIMENTO_EVIDENCE_DIR ?? '/tmp/pentimento-cef-evidence';
mkdirSync(out, { recursive: true });
const records = [];
let stage = 'launch';
let app;
let browser;
let page;
let sculptPoint;
let windowId;
let width;
let height;
let nativeInputs = 0;
const appLog = `${out}/app.log`;
function killNativeGroup(signal = 'SIGKILL') {
    if (app?.pid) { try { process.kill(-app.pid, signal); } catch {} }
}
// A timeout/cancel must also close the detached native process and its ephemeral
// debugging endpoint; JavaScript finally does not run after the default SIGTERM.
process.once('exit', () => killNativeGroup());
for (const signal of ['SIGTERM', 'SIGINT']) process.once(signal, () => {
    const result = { type: 'pentimento.cef.result', status: 'failed', stage, error: signal, records };
    writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
    console.error(JSON.stringify(result));
    killNativeGroup();
    process.exit(signal === 'SIGTERM' ? 143 : 130);
});

const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const command = (name, args, options = {}) => execFileSync(name, args.map(String), { cwd: root, maxBuffer: 20 * 1024 * 1024, ...options });
const xdo = (...args) => { nativeInputs++; return command('xdotool', args).toString().trim(); };
const log = () => { try { return readFileSync(appLog, 'utf8'); } catch { return ''; } };
const pointerTrace = () => log().split('\n').filter(line => /Native pointer order:|Native capture after batch:|Native input layout received:|Failed to parse.*IPC|Click at webview|Sculpt stroke started:/.test(line)).slice(-120);
const starts = () => ({ paint: (log().match(/StrokeStart: plane=/g) ?? []).length, sculpt: (log().match(/Sculpt stroke started:/g) ?? []).length });
const record = (name, evidence = {}) => {
    const item = { type: 'pentimento.cef.check', name, status: 'passed', native_inputs: nativeInputs, ...evidence };
    records.push(item); console.log(JSON.stringify(item));
};
async function until(check, description, timeout = 30000) {
    const end = Date.now() + timeout;
    while (Date.now() < end) {
        if (app && (app.exitCode !== null || app.signalCode !== null)) throw new Error(`Native app exited (${app.exitCode ?? app.signalCode}): ${description}`);
        try { const result = await check(); if (result) return result; } catch (error) { if (Date.now() + 200 >= end) throw error; }
        await pause(100);
    }
    throw new Error(`Timed out: ${description}`);
}
async function waitForStrokeCompletion(offset, description) {
    const started = await until(() => startedStroke(log().slice(offset)), `${description}: exact stroke start`);
    const completed = await until(() => completedStroke(log().slice(offset), started.id), `${description}: transaction ${started.id} completion`, 30000);
    assert.notEqual(completed.outcome, 'rejected', `${description}: transaction ${started.id} was rejected`);
    return completed;
}
async function capture(name, { synchronize = true } = {}) {
    // Move to empty viewport so brush gizmos/hover highlights cannot masquerade
    // as a geometry change in before/after viewport comparisons.
    const parkedLogOffset = log().length;
    xdo('mousemove', '--window', windowId, 20, height - 30);
    if (synchronize) await until(() => parkedPointerFrames(log().slice(parkedLogOffset), 20, height - 30),
        `${name}: off-target hover through gizmo/presentation update frames`, 30000);
    const pixels = command('import', ['-window', windowId, '-depth', '8', 'rgb:-']);
    assert.equal(pixels.length, width * height * 3, 'Unexpected X11 capture dimensions');
    command('convert', ['-size', `${width}x${height}`, '-depth', '8', 'rgb:-', '-quality', '85', `${out}/${name}.jpg`], { input: pixels });
    return { pixels, screenshot: `${name}.jpg`, sha256: createHash('sha256').update(pixels).digest('hex') };
}
function difference(a, b) {
    let changed = 0; let total = 0; let absolute = 0;
    // Exclude toolbar/FPS and side panels. The native viewport remains visible.
    for (let y = 70; y < height - 40; y++) for (let x = 30; x < width - 335; x++) {
        const index = (y * width + x) * 3;
        const delta = Math.max(...[0, 1, 2].map(channel => Math.abs(a.pixels[index + channel] - b.pixels[index + channel])));
        if (delta > 20) changed++;
        absolute += delta; total++;
    }
    return { changed_pixels: changed, sampled_pixels: total, mean_max_channel_difference: absolute / total };
}
async function clickAt(x, y) {
    xdo('mousemove', '--window', windowId, Math.round(x), Math.round(y));
    // Bevy Click targets the previous hover frame. Give native picking time to
    // establish hover and keep the press across frames on software-rendered CI.
    await pause(1000);
    let primaryError;
    try {
        xdo('mousedown', '1');
        await pause(1000);
    } catch (error) {
        primaryError = error;
        throw error;
    } finally {
        try { xdo('mouseup', '1'); } catch (error) {
            if (!primaryError) throw error;
            console.error(JSON.stringify({ type: 'pentimento.cef.input_release_failed', button: 1, error: String(error) }));
        }
    }
    await pause(250);
}
async function nativeClick(locator) {
    await locator.waitFor({ state: 'visible' });
    const box = await locator.boundingBox(); assert.ok(box);
    assertNativeClickBounds(box, width, height);
    await clickAt(box.x + box.width / 2, box.y + box.height / 2);
}
async function fill(locator, value) {
    await nativeClick(locator);
    xdo('key', '--clearmodifiers', 'ctrl+a');
    xdo('type', '--clearmodifiers', '--delay', '20', value);
    xdo('key', '--clearmodifiers', 'Return');
    await until(async () => await locator.inputValue() === value, `native field value ${value}`);
}
async function drag(x, y, dx, dy) {
    xdo('mousemove', '--window', windowId, Math.round(x), Math.round(y));
    xdo('mousedown', '1');
    for (let step = 1; step <= 16; step++) {
        xdo('mousemove', '--window', windowId, Math.round(x + dx * step / 16), Math.round(y + dy * step / 16));
        await pause(40);
    }
    xdo('mouseup', '1'); await pause(400);
}
async function key(value, observed, description) {
    const transport = {
        send: (action, key) => xdo(action, key), wait: until, pause,
        releaseFailed: error => console.error(JSON.stringify({ type: 'pentimento.cef.input_release_failed', key: value, error: String(error) })),
    };
    if (value === 'ctrl+Tab') return tapNativeShortcut('ctrl', 'Tab', observed, description, transport);
    return holdNativeKey(value, observed, description, transport);
}

async function failureDiagnostics() {
    if (!page) return null;
    let timer;
    try {
        return await Promise.race([page.evaluate(point => {
            const describe = element => element ? {
                tag: element.tagName, id: element.id, role: element.getAttribute('role'),
                classes: element.getAttribute('class'),
            } : null;
            return {
                width: innerWidth, height: innerHeight, pixel_ratio: devicePixelRatio,
                native_input_events: window.__PENTIMENTO_NATIVE_INPUT_EVIDENCE__ ?? null,
                active_element: describe(document.activeElement),
                active_input: document.activeElement instanceof HTMLInputElement ? {
                    type: document.activeElement.type,
                    value: document.activeElement.value,
                    value_as_number: Number.isFinite(document.activeElement.valueAsNumber)
                        ? document.activeElement.valueAsNumber : null,
                    bad_input: document.activeElement.validity.badInput,
                    range_overflow: document.activeElement.validity.rangeOverflow,
                    range_underflow: document.activeElement.validity.rangeUnderflow,
                    step_mismatch: document.activeElement.validity.stepMismatch,
                    selection_start: document.activeElement.selectionStart,
                    selection_end: document.activeElement.selectionEnd,
                } : null,
                sculpt_point: point,
                sculpt_hit_element: point ? describe(document.elementFromPoint(point.x, point.y)) : null,
                headings: [...document.querySelectorAll('h1,h2,h3')].map(element => element.textContent),
                alerts: [...document.querySelectorAll('[role="alert"]')].map(element => element.textContent),
                brush_panels: [...document.querySelectorAll('.brush-panel')].map(element => element.textContent),
                regions: [...document.querySelectorAll('.toolbar,.side-panel,.brush-panel,.add-menu-backdrop')].map(element => {
                    const rect = element.getBoundingClientRect();
                    return { ...describe(element), x: rect.x, y: rect.y, width: rect.width, height: rect.height };
                }),
            };
        }, sculptPoint ?? null), new Promise(resolve => {
            timer = setTimeout(() => resolve({ unavailable: 'Renderer diagnostics timed out' }), 2000);
        })]);
    } finally {
        clearTimeout(timer);
    }
}

try {
    const logFd = openSync(appLog, 'w');
    // CEF documents port=0 as ephemeral. Chromium's production socket factory
    // binds loopback only; verify that at runtime before using the endpoint.
    console.log(JSON.stringify({ type: 'pentimento.cef.configuration', cef_rendering: cefRendering, cef_args: cefRenderingArgs }));
    app = spawn('./launcher.sh', ['--run', '--frontend', 'cef', '--', '--remote-debugging-port=0', ...cefRenderingArgs], {
        cwd: root, detached: true, stdio: ['ignore', logFd, logFd],
        env: { ...process.env, RUST_LOG: 'info,pentimento::input::mouse=debug', PENTIMENTO_NATIVE_DIAGNOSTICS: '1', PENTIMENTO_LAUNCHER_STATE_ROOT: `${out}/state`, GDK_BACKEND: 'x11', LIBGL_ALWAYS_SOFTWARE: '1', WGPU_BACKEND: 'vulkan' },
    });
    const endpoint = await until(() => log().match(/DevTools listening on (ws:\/\/(?:127\.0\.0\.1|\[::1\]):\d+\/\S+)/)?.[1], 'ephemeral CEF DevTools endpoint', 120000);
    const url = new URL(endpoint);
    const sockets = command('ss', ['-H', '-ltn', `sport = :${url.port}`]).toString().trim();
    assert.ok(sockets, 'No listener found for CEF debugging port');
    for (const line of sockets.split('\n')) {
        const address = line.trim().split(/\s+/)[3];
        assert.ok(address === `127.0.0.1:${url.port}` || address === `[::1]:${url.port}`, `Non-loopback listener: ${address}`);
    }
    windowId = await until(() => command('xdotool', ['search', '--onlyvisible', '--pid', app.pid, '--name', '^Pentimento$']).toString().trim().split('\n')[0], 'native Pentimento window');
    const geometry = command('xdotool', ['getwindowgeometry', '--shell', windowId]).toString();
    width = Number(geometry.match(/^WIDTH=(\d+)$/m)[1]); height = Number(geometry.match(/^HEIGHT=(\d+)$/m)[1]);
    xdo('windowfocus', '--sync', windowId);
    browser = await chromium.connectOverCDP(endpoint);
    page = await until(() => browser.contexts().flatMap(context => context.pages()).find(page => page.url().startsWith('data:text/html')), 'actual CEF UI page');
    await page.getByRole('button', { name: 'Reset Camera', exact: true }).waitFor();
    // Read-only trace: no input dispatch, focus changes, or backend commands.
    await page.evaluate(() => {
        const evidence = { keys: [], focus: [] };
        window.__PENTIMENTO_NATIVE_INPUT_EVIDENCE__ = evidence;
        for (const type of ['keydown', 'keyup']) window.addEventListener(type, event => {
            evidence.keys.push({ type, key: event.key, code: event.code, ctrl: event.ctrlKey,
                shift: event.shiftKey, alt: event.altKey, meta: event.metaKey, repeat: event.repeat,
                target: event.target?.tagName ?? null, time: performance.now() });
            if (evidence.keys.length > 128) evidence.keys.shift();
        });
        document.addEventListener('focusin', event => {
            evidence.focus.push({ target: event.target?.tagName ?? null, id: event.target?.id ?? null, time: performance.now() });
            if (evidence.focus.length > 32) evidence.focus.shift();
        });
    });

    await until(() => page.evaluate(() => typeof window.ipc?.postMessage === 'function' && typeof window.__PENTIMENTO_RECEIVE__ === 'function'), 'real native bridge readiness');
    stage = 'cef_framebuffer_readiness';
    const framebuffer = await until(() => cefFramebufferReceipt(log()), 'CEF first painted framebuffer (DOM readiness alone is insufficient)', 30000);
    const startup = await capture('native-startup');
    record('native_startup', { screenshot: startup.screenshot, pixels_sha256: startup.sha256, width, height, cdp_bindings: sockets });
    const toolbarPixels = assertPaintedUiRegion(startup, width, height, await page.locator('header.toolbar').boundingBox(), 'toolbar');
    record('cef_framebuffer_ready', { ...framebuffer, ...toolbarPixels, screenshot: startup.screenshot });

    stage = 'sculpt_entry';
    // Exact projection of the default sphere center (2,.5,0), using the default
    // orbit target/orientation and Bevy 45-degree vertical FOV. No scene mutation.
    const camera = [8.66 * Math.cos(.615) * Math.sin(Math.PI / 4), 8.66 * Math.sin(.615), 8.66 * Math.cos(.615) * Math.cos(Math.PI / 4)];
    const delta = [2 - camera[0], .5 - camera[1], -camera[2]];
    const right = [Math.cos(Math.PI / 4), 0, -Math.sin(Math.PI / 4)];
    const up = [-Math.sin(.615) * Math.sin(Math.PI / 4), Math.cos(.615), -Math.sin(.615) * Math.cos(Math.PI / 4)];
    const forward = camera.map(value => -value / 8.66);
    const dot = (a, b) => a.reduce((sum, value, index) => sum + value * b[index], 0);
    const depth = dot(delta, forward); const focal = height / (2 * Math.tan(Math.PI / 8));
    const sx = width / 2 + focal * dot(delta, right) / depth;
    const sy = height / 2 - focal * dot(delta, up) / depth;
    sculptPoint = { x: sx, y: sy };
    const selectedBefore = (log().match(/Added entity .* to ID buffer with color/g) ?? []).length;
    await clickAt(sx, sy);
    await until(() => (log().match(/Added entity .* to ID buffer with color/g) ?? []).length > selectedBefore,
        'native mesh selection');
    await key('ctrl+Tab', () => log().includes('Entered sculpt mode for entity'), 'native sculpt mode entry');
    stage = 'sculpt_panel';
    await page.getByRole('heading', { name: 'Sculpt brushes' }).waitFor();
    await nativeClick(page.getByRole('button', { name: 'Grab', exact: true }));
    await until(() => page.getByRole('button', { name: 'Grab', exact: true }).getAttribute('aria-pressed').then(value => value === 'true'), 'backend Grab selection');
    await fill(page.getByRole('spinbutton', { name: 'Radius value', exact: true }), '0.8');
    await clickAt(35, height - 35); // Misses the sculpt target and clears widget focus.
    record('sculpt_panel_settings', { tool: 'Grab', radius: .8 });
    const panelFrame = await capture('sculpt-panel-painted');
    const panelPixels = assertPaintedUiRegion(panelFrame, width, height, await page.locator('[data-ui-region="brush-panel"]').boundingBox(), 'sculpt controls');
    record('sculpt_panel_painted', { ...panelPixels, screenshot: panelFrame.screenshot });

    stage = 'sculpt_widget_capture';
    const radiusLogOffset = log().length;
    const controlStarts = starts(); const beforeControl = await capture('sculpt-before-widget');
    const slider = await page.locator('#sculpt-radius').boundingBox();
    await drag(slider.x + slider.width / 2, slider.y + slider.height / 2, sx - slider.x - slider.width / 2, sy - slider.y - slider.height / 2);
    assert.deepEqual(starts(), controlStarts, 'UI widget drag started an underlying brush stroke');
    const acceptedRadius = await until(async () => {
        const radius = Number(await page.locator('#sculpt-radius').inputValue());
        const number = Number(await page.getByRole('spinbutton', { name: 'Radius value', exact: true }).inputValue());
        const receipt = [...log().slice(radiusLogOffset).matchAll(/Sculpt radius accepted: value=([\d.e+-]+) pipeline=Some\(([\d.e+-]+)\)/g)].at(-1);
        if (receipt && Math.abs(Number(receipt[1]) - radius) < 0.00001
            && Math.abs(Number(receipt[2]) - radius) < 0.00001 && radius === number) return radius;
        return false;
    }, 'sculpt slider, number and accepted pipeline radius agreement');
    const afterControl = await capture('sculpt-after-widget');
    const controlDelta = difference(beforeControl, afterControl);
    assert.ok(controlDelta.mean_max_channel_difference < 2, 'Sculpt geometry changed during UI drag');
    record('sculpt_widget_no_stroke', { ...controlDelta, accepted_radius: acceptedRadius, stroke_starts: starts(), screenshot: afterControl.screenshot, native_pointer_trace: pointerTrace() });
    await fill(page.getByRole('spinbutton', { name: 'Radius value', exact: true }), '0.8');
    await clickAt(35, height - 35);
    const beforeSculpt = await capture('sculpt-before-stroke'); const beforeStarts = starts();
    const beforeSculptFrames = [beforeSculpt, await capture('sculpt-before-noise-1'), await capture('sculpt-before-noise-2')];
    stage = 'sculpt_stroke';
    const sculptUndo = page.getByRole('button', { name: 'Undo sculpt stroke', exact: true });
    const sculptRedo = page.getByRole('button', { name: 'Redo sculpt stroke', exact: true });
    const sculptLogOffset = log().length;
    await drag(sx, sy, 24, -12);
    await until(() => starts().sculpt > beforeStarts.sculpt, 'native sculpt stroke start');
    const sculptCompletion = assertAcceptedStroke(await waitForStrokeCompletion(sculptLogOffset, 'visible sculpt'), 'visible sculpt');
    const { frame: afterSculpt, delta: sculptDelta } = await waitForSculptPresentation(until, {
        historyReady: () => sculptUndo.isEnabled(),
        capture: () => capture('sculpt-after-stroke'),
        measure: frame => difference(beforeSculpt, frame),
        description: `visible sculpt transaction ${sculptCompletion.id}`,
    });
    assert.ok(sculptDelta.changed_pixels > 25, 'Sculpt stroke produced no visible viewport change');
    const afterSculptFrames = [afterSculpt, await capture('sculpt-after-noise-1'), await capture('sculpt-after-noise-2')];
    const sculptRegion = deformationRegion(beforeSculptFrames, afterSculptFrames, width, height);
    record('sculpt_stroke', { ...sculptDelta, completion: sculptCompletion, stroke_starts: starts(), screenshot: afterSculpt.screenshot });
    stage = 'sculpt_history';
    await until(() => sculptUndo.isEnabled(), 'native sculpt history acceptance');
    const historyStarts = starts();
    await nativeClick(sculptUndo);
    await until(() => sculptRedo.isEnabled(), 'native sculpt undo');
    assert.deepEqual(starts(), historyStarts, 'Sculpt Undo button started an underlying stroke');
    const undoneSculpt = await capture('sculpt-undone');
    const sculptUndoDelta = difference(beforeSculpt, undoneSculpt);
    assert.ok(sculptUndoDelta.mean_max_channel_difference < 2, 'Sculpt Undo did not restore the visible baseline');
    const sculptUndoRegion = assertRegionRestored(sculptRegion, sculptRegion.baseline, sculptRegion.deformed, undoneSculpt, 'Sculpt Undo');
    record('sculpt_undo', { ...sculptUndoDelta, ...sculptUndoRegion, screenshot: undoneSculpt.screenshot });
    await nativeClick(sculptRedo);
    await until(() => sculptRedo.isDisabled(), 'native sculpt redo');
    const redoneSculpt = await capture('sculpt-redone');
    const sculptRedoDelta = difference(afterSculpt, redoneSculpt);
    assert.ok(sculptRedoDelta.mean_max_channel_difference < 2, 'Sculpt Redo did not restore the visible stroke');
    assert.ok(difference(beforeSculpt, redoneSculpt).changed_pixels > 25, 'Sculpt Redo has no visible deformation');
    const sculptRedoRegion = assertRegionRestored(sculptRegion, sculptRegion.deformed, undoneSculpt, redoneSculpt, 'Sculpt Redo');
    record('sculpt_redo', { ...sculptRedoDelta, ...sculptRedoRegion, screenshot: redoneSculpt.screenshot });
    const beforeCancelFrames = [redoneSculpt, await capture('sculpt-redone-noise-1'), await capture('sculpt-redone-noise-2')];
    let cancelRegion;
    const cancelStarts = starts();
    const cancelLogOffset = log().length;
    const rollbackCount = (log().match(/Sculpt stroke rollback:/g) ?? []).length;
    try {
        xdo('mousemove', '--window', windowId, Math.round(sx), Math.round(sy));
        xdo('mousedown', '1');
        await until(() => starts().sculpt > cancelStarts.sculpt, 'native cancellable sculpt stroke');
        for (let step = 1; step <= 8; step++) {
            xdo('mousemove', '--window', windowId, Math.round(sx - step), Math.round(sy + step));
            await pause(100);
        }
        // Establish visible deformation before Escape. Moving the pointer to
        // the same empty capture location hides the gizmo while the button stays held.
        const duringCancelFrames = [await capture('sculpt-cancel-active'), await capture('sculpt-cancel-active-noise-1'), await capture('sculpt-cancel-active-noise-2')];
        cancelRegion = deformationRegion(beforeCancelFrames, duringCancelFrames, width, height);
        xdo('key', '--clearmodifiers', 'Escape');
        await until(() => (log().match(/Sculpt stroke rollback:/g) ?? []).length > rollbackCount, 'native sculpt rollback');
        const cancellation = await waitForStrokeCompletion(cancelLogOffset, 'Escape rollback');
        assert.equal(cancellation.outcome, 'cancelled');
    } finally { xdo('mouseup', '1'); }
    const cancelledSculpt = await capture('sculpt-cancelled');
    const sculptCancelDelta = difference(redoneSculpt, cancelledSculpt);
    assert.ok(sculptCancelDelta.mean_max_channel_difference < 2, 'Sculpt Escape did not roll back visible geometry');
    const sculptCancelRegion = assertRegionRestored(cancelRegion, cancelRegion.baseline, cancelRegion.deformed, cancelledSculpt, 'Sculpt Escape rollback');
    record('sculpt_cancel_rollback', { ...sculptCancelDelta, ...sculptCancelRegion, active_screenshot: 'sculpt-cancel-active.jpg', screenshot: cancelledSculpt.screenshot });
    await nativeClick(sculptUndo);
    await until(() => sculptRedo.isEnabled(), 'native sculpt branch baseline');
    const branchLogOffset = log().length;
    await drag(sx, sy, -16, 8);
    assertAcceptedStroke(await waitForStrokeCompletion(branchLogOffset, 'new history branch'), 'new history branch');
    await until(async () => (await sculptUndo.isEnabled()) && (await sculptRedo.isDisabled()), 'new accepted sculpt branch clears redo');
    const { frame: branchedSculpt } = await waitForSculptPresentation(until, {
        historyReady: async () => (await sculptUndo.isEnabled()) && (await sculptRedo.isDisabled()),
        capture: () => capture('sculpt-new-branch'),
        measure: frame => difference(beforeSculpt, frame),
        description: 'new accepted history branch',
    });
    assert.ok(difference(beforeSculpt, branchedSculpt).changed_pixels > 25, 'New sculpt branch has no visible deformation');
    record('sculpt_new_branch', { screenshot: branchedSculpt.screenshot });

    await key('Tab'); assert.equal(await page.getByRole('heading', { name: 'Sculpt brushes' }).count(), 1);
    // Tab may focus a browser widget. Return keyboard ownership to the viewport
    // before its Ctrl+Tab shortcut; a miss in sculpt mode preserves selection.
    await clickAt(35, height - 35);
    stage = 'sculpt_mode_ownership';
    const exitLogOffset = log().length;
    await key('ctrl+Tab', () => log().slice(exitLogOffset).includes('Exited sculpt mode'), 'native sculpt exit receipt');
    await until(() => page.getByRole('heading', { name: 'Sculpt brushes' }).count().then(n => n === 0), 'sculpt exit');
    const reentryLogOffset = log().length;
    await key('ctrl+Tab', () => log().slice(reentryLogOffset).includes('Entered sculpt mode for entity'), 'native sculpt reentry receipt');
    await page.getByRole('heading', { name: 'Sculpt brushes' }).waitFor();
    await until(async () => Number(await page.locator('#sculpt-radius').inputValue()) === .8
        && Number(await page.getByRole('spinbutton', { name: 'Radius value', exact: true }).inputValue()) === .8,
    'backend sculpt radius persists through exit and reentry');
    const finalExitLogOffset = log().length;
    await key('ctrl+Tab', () => log().slice(finalExitLogOffset).includes('Exited sculpt mode'), 'native final sculpt exit receipt');
    await until(() => page.getByRole('heading', { name: 'Sculpt brushes' }).count().then(n => n === 0), 'final sculpt exit');
    record('sculpt_mode_ownership', { plain_tab_preserved_sculpt: true, exit_reentry: true, screenshot: (await capture('sculpt-after-exit')).screenshot });

    stage = 'paint_entry';
    await clickAt(35, height - 35); await key('shift+a');
    await page.getByRole('dialog', { name: 'Add Object', exact: true }).waitFor();
    const menuFrame = await capture('native-add-menu-open');
    const menuBounds = await page.locator('.add-menu').boundingBox();
    assertNativeClickBounds(menuBounds, width, height);
    record('native_open_menu', { ...assertPaintedUiRegion(menuFrame, width, height, menuBounds, 'Add Object menu'), bounds: menuBounds, screenshot: menuFrame.screenshot });
    await nativeClick(page.getByRole('button', { name: 'Paint', exact: true }));
    await page.getByRole('heading', { name: 'Projection paint' }).waitFor();
    await fill(page.getByRole('textbox', { name: 'Hex color', exact: true }), '#ff00ff');
    await fill(page.getByRole('spinbutton', { name: 'Radius value', exact: true }), '24');
    await clickAt(35, height - 35); // Outside canvas bounds and any panel.
    const undo = page.getByRole('button', { name: 'Undo canvas stroke', exact: true });
    assert.equal(await undo.isDisabled(), true);
    stage = 'paint_widget_capture';
    const paintControlStarts = starts(); const paintBeforeControl = await capture('paint-before-widget');
    const paintSlider = await page.locator('#paint-radius').boundingBox();
    await drag(paintSlider.x + paintSlider.width / 2, paintSlider.y + paintSlider.height / 2,
        width * .43 - paintSlider.x - paintSlider.width / 2, height * .52 - paintSlider.y - paintSlider.height / 2);
    assert.deepEqual(starts(), paintControlStarts, 'Paint widget drag started an underlying brush stroke');
    assert.equal(await undo.isDisabled(), true);
    const paintAfterControl = await capture('paint-after-widget'); const paintControlDelta = difference(paintBeforeControl, paintAfterControl);
    assert.ok(paintControlDelta.mean_max_channel_difference < 2, 'Canvas pixels changed during UI drag');
    record('paint_widget_no_stroke', { ...paintControlDelta, stroke_starts: starts(), screenshot: paintAfterControl.screenshot });
    await fill(page.getByRole('spinbutton', { name: 'Radius value', exact: true }), '24');
    await clickAt(35, height - 35);
    stage = 'paint_stroke';
    const beforePaint = await capture('paint-before-stroke'); const paintStarts = starts();
    await drag(width * .43, height * .52, 90, -25);
    await until(() => undo.isDisabled().then(disabled => !disabled), 'canvas undo availability');
    assert.ok(starts().paint > paintStarts.paint, 'No engine paint stroke received');
    const afterPaint = await capture('paint-after-stroke'); const paintDelta = difference(beforePaint, afterPaint);
    assert.ok(paintDelta.changed_pixels > 100, 'Paint stroke produced no visible viewport change');
    const continuousPaint = assertContinuousPaint(beforePaint, afterPaint, width, height,
        { x: width * .43, y: height * .52 }, { x: width * .43 + 90, y: height * .52 - 25 });
    record('paint_stroke', { ...paintDelta, ...continuousPaint, stroke_starts: starts(), screenshot: afterPaint.screenshot });
    const beforeUndo = starts(); await nativeClick(undo);
    await until(() => undo.isDisabled(), 'canvas undo restored empty history');
    assert.deepEqual(starts(), beforeUndo, 'Clicking Undo also started a canvas stroke');
    const afterUndo = await capture('paint-after-undo'); const undoDelta = difference(beforePaint, afterUndo);
    assert.ok(undoDelta.mean_max_channel_difference < 2, 'Undo did not restore visible source canvas');
    record('paint_undo_and_ui_capture', { ...undoDelta, screenshot: afterUndo.screenshot });
    // Project a nonempty real source stroke after proving actual canvas Undo.
    await drag(width * .43, height * .52, 90, -25);
    await until(() => undo.isEnabled(), 'second canvas stroke committed before projection');
    const projectionSource = await capture('paint-projection-source');
    assertContinuousPaint(afterUndo, projectionSource, width, height,
        { x: width * .43, y: height * .52 }, { x: width * .43 + 90, y: height * .52 - 25 });
    await nativeClick(page.getByRole('checkbox', { name: 'Live projection', exact: true }));
    await until(() => page.getByRole('checkbox', { name: 'Live projection', exact: true }).isChecked(), 'live projection backend state');
    await nativeClick(page.getByRole('button', { name: 'Apply canvas to UV surfaces', exact: true }));
    await until(() => log().includes('Live projection enabled') && log().includes('Project to scene requested'), 'production projection event handlers');
    const projectionFrame = await capture('paint-after-projection');
    record('projection_controls', { live_projection: true, applied: true, engine_event_log: true,
        nonempty_source: true, source_screenshot: projectionSource.screenshot, screenshot: projectionFrame.screenshot });

    stage = 'complete';
    writeFileSync(`${out}/result.json`, JSON.stringify({ type: 'pentimento.cef.result', status: 'passed', commit: command('git', ['rev-parse', 'HEAD']).toString().trim(), records, native_input_events: (await failureDiagnostics())?.native_input_events ?? null }, null, 2));
    console.log(JSON.stringify({ type: 'pentimento.cef.result', status: 'passed', checks: records.length }));
} catch (error) {
    if (windowId && width && height) { try { await capture('failure', { synchronize: false }); } catch {} }
    let diagnostics = null;
    try { diagnostics = await failureDiagnostics(); } catch {}
    const nativePointerTrace = pointerTrace();
    const result = { type: 'pentimento.cef.result', status: 'failed', stage, error: String(error), records, diagnostics, native_pointer_trace: nativePointerTrace };
    writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2)); console.error(JSON.stringify(result)); process.exitCode = 1;
} finally {
    if (browser) await browser.close().catch(() => {});
    if (app?.pid) {
        try { process.kill(-app.pid, 'SIGTERM'); } catch {}
        await pause(1000);
        try { process.kill(-app.pid, 'SIGKILL'); } catch {}
    }
}
