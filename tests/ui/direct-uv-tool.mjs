// Actual Svelte controls -> shared Rust dispatcher -> production Bevy CPU assets.
// The diagnostic texture below is not native CEF or GPU-shaded acceptance.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { mkdir, writeFile, copyFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const output = process.env.PENTIMENTO_DIRECTUV_EVIDENCE;
assert.ok(output, 'PENTIMENTO_DIRECTUV_EVIDENCE is required; evidence stays outside Git');
await mkdir(`${output}/driver-fixtures`, { recursive: true });
assert.ok(process.env.PENTIMENTO_DIRECTUV_DRIVER_BIN, 'build the qualified app or scene test binary first');
const driver = spawn(process.env.PENTIMENTO_DIRECTUV_DRIVER_BIN,
  [process.env.PENTIMENTO_DIRECTUV_DRIVER_TEST ?? 'direct_uv_tool_tests::browser_driver', '--exact', '--ignored', '--nocapture', '--test-threads=1'],
  { env: { ...process.env, PENTIMENTO_DIRECTUV_DRIVER_DIR: `${output}/driver-fixtures`, PENTIMENTO_BRUSH_PRESETS_PATH: `${output}/driver-fixtures/brushes.json` }, stdio: ['pipe', 'pipe', 'pipe'] });
const waiting = [];
let initialResolve, initialReject;
const initial = new Promise((resolve, reject) => { initialResolve = resolve; initialReject = reject; });
const initialTimer = setTimeout(() => initialReject?.(new Error('Rust handshake timed out')), 30000);
let stderr = '';
const fail = error => {
  clearTimeout(initialTimer); initialReject?.(error); initialReject = null;
  for (const job of waiting.splice(0)) { clearTimeout(job.timer); job.reject(error); }
};
driver.stderr.on('data', b => { stderr += b; });
driver.on('error', fail);
driver.stdin.on('error', fail);
createInterface({ input: driver.stdout }).on('line', line => {
  const index = line.indexOf('PENTIMENTO_DRIVER ');
  if (index < 0) return;
  const state = JSON.parse(line.slice(index + 'PENTIMENTO_DRIVER '.length));
  if (initialResolve) { clearTimeout(initialTimer); initialResolve(state); initialResolve = null; initialReject = null; }
  else { const job = waiting.shift(); if (job) { clearTimeout(job.timer); job.resolve(state); } }
});
driver.on('exit', code => { if (initialReject || waiting.length || code !== 0) fail(new Error(`Rust driver exited ${code}: ${stderr}`)); });
let queue = Promise.resolve();
const send = request => {
  const response = queue.then(() => new Promise((resolve, reject) => {
    const timer = setTimeout(() => fail(new Error('Rust request timed out')), 30000);
    waiting.push({ resolve, reject, timer }); driver.stdin.write(`${JSON.stringify(request)}\n`);
  }));
  queue = response.then(() => undefined, () => undefined); return response;
};
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? 'playwright-core');
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH ?? '/usr/bin/chromium', headless: true, args: ['--no-sandbox'] });
let last;
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  page.setDefaultTimeout(15000);
  const errors = []; page.on('pageerror', e => { errors.push(e.message); console.error(e.message); });
  const first = await initial;
  await page.exposeFunction('__DIRECTUV_SEND__', send);
  await page.addInitScript(() => {
    window.commands = [];
    window.acceptDirect = state => {
      window.directState = state;
      for (const message of state.messages) window.__PENTIMENTO_RECEIVE__?.(JSON.stringify(message));
      const canvas = document.getElementById('qualified-texture');
      if (canvas) canvas.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(state.image), 64, 64), 0, 0);
    };
    window.__PENTIMENTO_IPC__ = { postMessage: raw => {
      const ui = JSON.parse(raw); window.commands.push(ui);
      // UiDirty is a renderer repaint marker; this CPU transport has no GPU frame loop.
      if (ui.type === "UiDirty") return;
      window.__DIRECTUV_SEND__({ ui }).then(window.acceptDirect);
    } };
  });
  await page.goto(process.env.PENTIMENTO_UI_URL ?? 'http://127.0.0.1:5203');
  await page.waitForFunction(() => window.directState, undefined, { timeout: 30000 });
  // This external diagnostic is a test surface, never shipped as product UI.
  await page.evaluate(() => {
    const panel = document.createElement('div');
    panel.style.cssText = 'position:fixed;left:90px;top:180px;z-index:10;width:480px;color:white;background:#202329;padding:18px;font:14px sans-serif';
    panel.innerHTML = '<p>DirectUV CPU texture — real Rust backend<br>Native CEF / GPU shading unqualified</p><canvas id="qualified-texture" width="64" height="64" tabindex="0" style="width:440px;height:440px;image-rendering:pixelated"></canvas>';
    document.body.append(panel);
    const canvas = panel.querySelector('canvas');
    const pointer = event => {
      const kind = { pointermove: 'move', pointerdown: 'down', pointerup: 'up', pointercancel: 'cancel' }[event.type];
      const rect = canvas.getBoundingClientRect();
      // The fixture camera uses a 1000px viewport and a horizontal y=500 ray.
      const x = (event.clientX - rect.left) / rect.width * 1000;
      if (kind === 'down') { canvas.focus(); if (event.isTrusted) canvas.setPointerCapture(event.pointerId); }
      window.__DIRECTUV_SEND__({ pointer: { kind, x, pointerType: event.pointerType, pressure: event.pressure, id: event.pointerId } }).then(window.acceptDirect);
    };
    for (const type of ['pointermove', 'pointerdown', 'pointerup', 'pointercancel']) canvas.addEventListener(type, pointer);
    for (const type of ['keydown', 'keyup']) canvas.addEventListener(type, event => {
      if (['Escape', 'ShiftLeft', 'Tab', 'ControlLeft', 'KeyZ'].includes(event.code)) {
        event.preventDefault(); event.stopPropagation();
        window.__DIRECTUV_SEND__({ key: { code: event.code, down: type === 'keydown' } }).then(window.acceptDirect);
      }
    });
    window.acceptDirect(window.directState);
  });
  const settle = async () => {
    await page.evaluate(async () => window.acceptDirect(await window.__DIRECTUV_SEND__({ ui: { type: 'RequestBrushState' } })));
    last = await page.evaluate(() => window.directState); return last;
  };
  const checkpoint = async () => {
    const state = await settle(); return { bits: state.pixel_bits, image: state.image };
  };
  const equal = (a, b) => { assert.deepEqual(a.bits, b.bits); assert.deepEqual(a.image, b.image); };
  const field = async (label, value) => {
    const input = page.getByRole('spinbutton', { name: label, exact: true });
    await input.fill(String(value)); await input.press('Tab'); await settle();
  };
  await page.getByRole('button', { name: 'DirectUV surface', exact: true }).click();
  await settle(); await page.getByRole('heading', { name: 'DirectUV paint', exact: true }).waitFor();
  assert.equal(await page.getByRole('button', { name: 'Sample canvas color', exact: true }).count(), 0);
  await field('Radius value', 5); await field('Opacity value', 45);
  await field('Hardness / falloff value', 70); await field('Dab spacing value', 15);
  const color = async hex => { const input = page.getByLabel('Hex color', { exact: true }); await input.fill(hex); await input.press('Tab'); await settle(); };
  await color('#ff5522');
  await page.getByLabel('Paint preset name', { exact: true }).fill('DirectUV layered tip');
  await page.getByRole('button', { name: 'Save paint brush', exact: true }).click(); await settle();
  await field('Radius value', 8);
  await page.getByLabel('Saved paint brushes', { exact: true }).selectOption({ label: 'DirectUV layered tip' });
  await page.getByRole('button', { name: 'Use paint brush', exact: true }).click(); await settle();
  assert.equal(await page.getByRole('spinbutton', { name: 'Radius value', exact: true }).inputValue(), '5');
  await page.getByRole('heading',{name:'DirectUV paint',exact:true}).scrollIntoViewIfNeeded();
  await page.screenshot({path:`${output}/directuv-mode-brush.jpg`,type:'jpeg',quality:85});
  const canvas = page.locator('#qualified-texture');
  const box = await canvas.boundingBox();
  const move = x => page.mouse.move(box.x + box.width * x, box.y + box.height / 2);
  const stroke = async () => { await move(.36); await page.mouse.down(); for (const x of [.4,.45,.5,.55,.6,.64]) await move(x); await page.mouse.up(); await settle(); };
  const baseline = await checkpoint(); await stroke(); const one = await checkpoint();
  assert.notDeepEqual(one.bits, baseline.bits); assert.equal(last.undo, 1);
  await color('#2255ff'); await stroke(); const two = await checkpoint();
  assert.notDeepEqual(two.bits, one.bits); assert.equal(last.undo, 2);
  await page.getByRole('button', { name: 'Undo surface stroke', exact: true }).click(); equal(await checkpoint(), one);
  await page.getByRole('button', { name: 'Redo surface stroke', exact: true }).click(); equal(await checkpoint(), two);
  await canvas.focus(); await page.keyboard.press('Control+z'); equal(await checkpoint(), one);
  await page.keyboard.press('Control+Shift+z'); equal(await checkpoint(), two);
  await move(.45); await page.mouse.down(); await move(.55); await settle();
  for (const name of ['Canvas projection','DirectUV surface','Brush','Eraser','Undo surface stroke','Redo surface stroke','Save paint brush','Use paint brush']) {
    assert.equal(await page.getByRole('button',{name,exact:true}).isDisabled(),true,`${name} locked during actual stroke`);
  }
  for (const name of ['Brush preset','Saved paint brushes','Hex color']) assert.equal(await page.getByLabel(name,{exact:true}).isDisabled(),true);
  for (const name of ['Radius value','Opacity value','Hardness / falloff value','Dab spacing value']) assert.equal(await page.getByRole('spinbutton',{name,exact:true}).isDisabled(),true);
  for (const id of ['paint-radius','paint-opacity','paint-hardness','paint-spacing','paint-color']) assert.equal(await page.locator(`#${id}`).isDisabled(),true);
  assert.equal(await page.getByRole('button',{name:'Cancel current stroke',exact:true}).isDisabled(),false);
  await page.keyboard.press('Escape'); await page.mouse.up(); equal(await checkpoint(), two); assert.equal(last.undo, 2);
  await move(.45); await page.mouse.down(); await move(.55); await settle();
  // Activate the real Cancel control without leaving the scene with the held pointer.
  await page.getByRole('button', { name: 'Cancel current stroke', exact: true }).focus();
  await page.keyboard.press('Enter'); await page.mouse.up(); equal(await checkpoint(), two);
  const file = () => page.getByRole('button', { name: 'File', exact: true }).click();
  await file(); await page.getByRole('menuitem', { name: 'Save', exact: true }).click();
  await page.getByLabel('Absolute local file path').fill(first.project_path);
  await page.getByRole('button', { name: 'Save', exact: true }).click(); await settle();
  assert.equal(await page.getByRole('dialog').count(), 0);
  await copyFile(first.project_path, `${output}/owned-saved.pentimento.json`);
  await page.screenshot({ path: `${output}/directuv-layered-saved.jpg`, type: 'jpeg', quality: 85 });
  await file(); await page.getByRole('menuitem', { name: 'Open...' }).click();
  await page.getByRole('button', { name: 'Open', exact: true }).click(); equal(await checkpoint(), two);
  assert.equal(last.undo, 0); assert.equal(last.redo, 0);
  // Re-entry is a native window-key scenario, not an invented UI receipt.
  await canvas.focus(); await page.keyboard.press('Shift+Tab'); await settle();
  await page.getByRole('button', { name: 'DirectUV surface', exact: true }).click(); await settle();
  equal(await checkpoint(), two); await stroke(); const reopened = await checkpoint();
  assert.notDeepEqual(reopened.bits, two.bits); assert.equal(last.undo, 1);
  await page.getByRole('button', { name: 'Undo surface stroke', exact: true }).click(); equal(await checkpoint(), two);
  await page.screenshot({ path: `${output}/directuv-reopened-undo.jpg`, type: 'jpeg', quality: 85 });
  const penChecks = [];
  if (first.native_input_forwarding) {
    const pen = async (phase,x,pressure) => {
      await canvas.dispatchEvent(`pointer${phase}`,{pointerType:'pen',pointerId:17,isPrimary:true,clientX:box.x+box.width*x,clientY:box.y+box.height/2,pressure});
      await settle();
    };
    await pen('down',.4,.25);
    assert.equal(await page.getByRole('button',{name:'Cancel current stroke',exact:true}).isDisabled(),false);
    await pen('move',.5,.5); await pen('up',.6,.7); const penOne=await checkpoint();
    assert.notDeepEqual(penOne.bits,two.bits); assert.equal(last.undo,1); assert.equal(last.redo,0);
    await color('#00cc99'); await pen('down',.4,.5); await pen('move',.5,.8); await pen('up',.6,.8); const penTwo=await checkpoint();
    assert.notDeepEqual(penTwo.bits,penOne.bits); assert.equal(last.undo,2);
    await page.getByRole('button',{name:'Undo surface stroke',exact:true}).click(); equal(await checkpoint(),penOne);
    await page.getByRole('button',{name:'Redo surface stroke',exact:true}).click(); equal(await checkpoint(),penTwo);
    await pen('down',.45,.7); await pen('move',.55,.8); await pen('cancel',.55,0.); equal(await checkpoint(),penTwo); assert.equal(last.undo,2);
    await page.screenshot({path:`${output}/directuv-native-pen-history.jpg`,type:'jpeg',quality:85});
    penChecks.push('native-app-held-pressure-contact','native-app-layered-pen-Undo-Redo','native-app-pointercancel');
  }
  assert.deepEqual(errors, []);
  const hash = value => createHash('sha256').update(JSON.stringify(value)).digest('hex');
  const report = { status: 'passed', qualification: first.native_input_forwarding ? 'real Svelte controls and DOM pointer samples, existing native app UI dispatcher and input forwarding, recording CompositeBackend, production CPU image assets and owned file I/O' : 'real Svelte controls, shared scene dispatcher, production Bevy window input and CPU image assets, owned file I/O', native_cef_gpu: false, native_input_forwarding: !!first.native_input_forwarding,
    checks: [...penChecks, 'supported-mode', 'shared-brush-settings-and-catalog', 'two-layered-strokes', 'exact-float-and-display-Undo-Redo', 'chronological-keyboard-history', 'Escape-cancel', 'Cancel-control', 'active-control-gating', 'owned-save-open', 'fresh-history-and-post-open-edit'],
    baseline: hash(baseline), first_stroke: hash(one), layered: hash(two), reopened_stroke: hash(reopened) };
  await writeFile(`${output}/browser-directuv-report.json`, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify(report));
} finally {
  await browser.close(); if (driver.exitCode === null) driver.stdin.end('{"stop":true}\n');
}
