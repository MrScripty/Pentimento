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
  [process.env.PENTIMENTO_DIRECTUV_DRIVER_TEST ?? 'input::direct_uv_native_tests::browser_driver', '--exact', '--ignored', '--nocapture', '--test-threads=1'],
  { env: { ...process.env, PENTIMENTO_UV_LAYERS_DRIVER: '1', PENTIMENTO_DIRECTUV_DRIVER_DIR: `${output}/driver-fixtures`, PENTIMENTO_BRUSH_PRESETS_PATH: `${output}/driver-fixtures/brushes.json` }, stdio: ['pipe', 'pipe', 'pipe'] });
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
  assert.equal(first.native_input_forwarding,true,'shared layers qualification requires actual app native input driver');
  await page.exposeFunction('__DIRECTUV_SEND_JSON__', async request=>JSON.stringify(await send(request)));
  await page.addInitScript(() => {
    window.commands = [];
    window.__DIRECTUV_SEND__=async request=>JSON.parse(await window.__DIRECTUV_SEND_JSON__(request));
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
  await page.goto(process.env.PENTIMENTO_UI_URL ?? 'http://127.0.0.1:5207');
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
    last = JSON.parse(await page.evaluate(() => JSON.stringify(window.directState))); return last;
  };
  const checkpoint = async () => {
    const state = await settle(); return { bits: state.pixel_bits, image: state.image };
  };
  const equal = (a, b) => { assert.deepEqual(a.bits, b.bits); assert.deepEqual(a.image, b.image); };
  const field = async (label, value) => {
    const input = page.getByRole('spinbutton', { name: label, exact: true });
    await input.fill(String(value)); await input.press('Tab'); await settle();
  };

  const uv = async name => { await page.getByRole('button', {name,exact:true}).click(); await settle(); };
  await page.getByLabel('Paint receiver',{exact:true}).selectOption({label:'UV12'}); await settle();
  await uv('Enable UV texture layers');
  const baseline = last.uv_layers_document;
  await page.getByLabel('New UV layer name',{exact:true}).fill('Detail'); await uv('Create UV layer');
  let detail = last.uv_layers_document.active_layer;
  await uv('DirectUV surface');
  await field('Radius value',5); await field('Opacity value',45);
  const color = async hex => { const input=page.getByLabel('Hex color',{exact:true}); await input.fill(hex); await input.press('Tab'); await settle(); };
  await color('#2244ff');
  const canvas=page.locator('#qualified-texture');
  const box=await canvas.boundingBox();
  const move=x=>page.mouse.move(box.x+box.width*x,box.y+box.height/2);
  const stroke=async()=>{ await move(.36);await page.mouse.down();for(const x of [.4,.45,.5,.55,.6,.64]) await move(x);await page.mouse.up();await settle(); };
  await stroke();
  const direct=last.uv_layers_document;
  assert.notDeepEqual(direct.layers[1].pixels,baseline.layers[0].pixels);
  assert.deepEqual(direct.layers[0].pixels,baseline.layers[0].pixels);
  await uv('Undo UV layer edit'); assert.deepEqual(last.uv_layers_document.layers[1].pixels,baseline.layers[0].pixels);
  await uv('UV Layer 1'); assert.equal(last.redo,1);
  await uv('Redo UV layer edit'); assert.deepEqual(last.uv_layers_document,direct);
  await uv('Duplicate UV layer');
  await page.getByLabel('UV layer name',{exact:true}).fill('Soft copy'); await page.getByLabel('UV layer name',{exact:true}).press('Tab');await settle();
  const authoritative=last.uv_layers_document;
  await page.getByLabel('UV layer name',{exact:true}).fill('');await page.getByLabel('UV layer name',{exact:true}).press('Tab');await settle();
  assert.equal(await page.getByLabel('UV layer name',{exact:true}).inputValue(),'Soft copy');assert.deepEqual(last.uv_layers_document,authoritative);
  await page.getByLabel('UV layer opacity (%)',{exact:true}).fill('150');await page.getByLabel('UV layer opacity (%)',{exact:true}).press('Tab');await settle();
  assert.equal(await page.getByLabel('UV layer opacity (%)',{exact:true}).inputValue(),'100');assert.deepEqual(last.uv_layers_document,authoritative);
  await page.getByLabel('UV layer opacity (%)',{exact:true}).fill('25');await page.getByLabel('UV layer opacity (%)',{exact:true}).press('Tab');await settle();
  await page.getByLabel('Show UV layer Detail',{exact:true}).click();await settle();
  await page.getByLabel('Lock UV layer Soft copy',{exact:true}).click();await settle();
  const locked=last.uv_layers_document;await stroke();assert.deepEqual(last.uv_layers_document,locked);
  await uv('Lower UV layer'); await uv('Lower UV layer');
  assert.equal(last.uv_layers_document.layers[0].meta.name,'Soft copy');
  const bottomOrder=last.uv_layers_document;await uv('Raise UV layer');assert.equal(last.uv_layers_document.layers[1].meta.name,'Soft copy');await uv('Lower UV layer');assert.deepEqual(last.uv_layers_document,bottomOrder);
  const reordered=last.uv_layers_document;await uv('Delete UV layer');await uv('Undo UV layer edit');assert.deepEqual(last.uv_layers_document,reordered);
  console.log('qualified layer structure and refusal controls');
  await page.screenshot({path:`${output}/uv-layers-structure.jpg`,type:'jpeg',quality:85});
  await uv('Canvas projection');
  await uv('UV Layer 1');
  await color('#ff5522');await stroke();
  const source=last.source;const beforeApply=last.uv_layers_document;
  assert.equal(await page.getByLabel('Live UV preview',{exact:true}).isDisabled(),false);
  await uv('Apply canvas to active UV layer');
  let applied=last.uv_layers_document;
  assert.notDeepEqual(applied.layers[1].pixels,beforeApply.layers[1].pixels);
  assert.deepEqual(applied.layers[0],beforeApply.layers[0]);
  assert.deepEqual(applied.layers[2],beforeApply.layers[2]);
  await uv('Undo UV layer edit');assert.deepEqual(last.uv_layers_document,beforeApply);assert.deepEqual(last.source,source);
  await uv('Redo UV layer edit');assert.deepEqual(last.uv_layers_document,applied);
  const modeControl=page.getByLabel('UV layer blend mode',{exact:true});
  for(const mode of ['Multiply','Screen','Overlay']) {
    const before=last.uv_layers_document;const beforeImage=last.image;
    await modeControl.selectOption(mode);await settle();
    const blended=last.uv_layers_document;const blendedImage=last.image;
    assert.equal(await modeControl.inputValue(),mode);
    assert.equal(blended.layers.find(l=>l.meta.id===blended.active_layer).meta.blend_mode,mode);
    assert.deepEqual(blended.layers.map(l=>l.pixels),before.layers.map(l=>l.pixels));
    assert.notDeepEqual(blendedImage,beforeImage);
    await uv('Undo UV layer edit');assert.deepEqual(last.uv_layers_document,before);assert.deepEqual(last.image,beforeImage);
    await uv('Redo UV layer edit');assert.deepEqual(last.uv_layers_document,blended);assert.deepEqual(last.image,blendedImage);
  }
  // Combined blend metadata edits exercise the actual controls and derived CPU image.
  const blendBaseline=last.uv_layers_document;const blendBaselineImage=last.image;
  await page.getByLabel('UV layer opacity (%)',{exact:true}).fill('37');
  await page.getByLabel('UV layer opacity (%)',{exact:true}).press('Tab');await settle();
  const blendFaded=last.uv_layers_document;const blendFadedImage=last.image;
  assert.notDeepEqual(blendFadedImage,blendBaselineImage);
  assert.deepEqual(blendFaded.layers.map(l=>l.pixels),blendBaseline.layers.map(l=>l.pixels));
  await uv('Lower UV layer');
  const blendReordered=last.uv_layers_document;const blendReorderedImage=last.image;
  assert.notDeepEqual(blendReorderedImage,blendFadedImage);
  for(const layer of blendBaseline.layers)assert.deepEqual(blendReordered.layers.find(l=>l.meta.id===layer.meta.id).pixels,layer.pixels);
  await uv('Undo UV layer edit');assert.deepEqual(last.uv_layers_document,blendFaded);assert.deepEqual(last.image,blendFadedImage);
  await uv('Undo UV layer edit');assert.deepEqual(last.uv_layers_document,blendBaseline);assert.deepEqual(last.image,blendBaselineImage);
  await uv('Redo UV layer edit');await uv('Redo UV layer edit');
  assert.deepEqual(last.uv_layers_document,blendReordered);assert.deepEqual(last.image,blendReorderedImage);
  await uv('Undo UV layer edit');await uv('Undo UV layer edit');
  assert.deepEqual(last.uv_layers_document,blendBaseline);assert.deepEqual(last.image,blendBaselineImage);
  applied=last.uv_layers_document;
  await page.screenshot({path:`${output}/uv-layers-shared-apply.jpg`,type:'jpeg',quality:85});
  const live=page.getByLabel('Live UV preview',{exact:true});
  const accepted=await checkpoint();const historyCount=last.undo;
  await live.click();await settle();assert.deepEqual(last.uv_layers_document,applied);assert.equal(last.undo,historyCount);
  assert.equal(await page.getByRole('button',{name:'DirectUV surface',exact:true}).isDisabled(),true);
  assert.equal(await page.getByRole('button',{name:'Create UV layer',exact:true}).isDisabled(),true);
  assert.equal(await modeControl.isDisabled(),true);
  const firstPreview=await checkpoint();assert.notDeepEqual(firstPreview.bits,accepted.bits);
  await color('#00cc55');await stroke();const afterSource=last.source;const secondPreview=await checkpoint();
  assert.notDeepEqual(secondPreview.bits,firstPreview.bits);assert.deepEqual(last.uv_layers_document,applied);
  await uv('Undo canvas stroke');equal(await checkpoint(),firstPreview);
  await uv('Redo canvas stroke');equal(await checkpoint(),secondPreview);assert.deepEqual(last.source,afterSource);
  await page.screenshot({path:`${output}/uv-live-preview.jpg`,type:'jpeg',quality:85});
  await uv('Cancel UV preview');equal(await checkpoint(),accepted);assert.deepEqual(last.source,afterSource);assert.equal(last.undo,historyCount);
  await live.click();await settle();const commitPreview=await checkpoint();await uv('Apply canvas to active UV layer');equal(await checkpoint(),commitPreview);
  assert.equal(last.undo,historyCount+1);assert.equal(await live.isChecked(),false);applied=last.uv_layers_document;
  await uv('Undo UV layer edit');equal(await checkpoint(),accepted);assert.deepEqual(last.source,afterSource);
  await uv('Redo UV layer edit');equal(await checkpoint(),commitPreview);
  const file=()=>page.getByRole('button',{name:'File',exact:true}).click();
  await file();await page.getByRole('menuitem',{name:'Save',exact:true}).click();await page.getByLabel('Absolute local file path').fill(first.project_path);
  await page.getByRole('button',{name:'Save',exact:true}).click();await settle();
  await copyFile(first.project_path,`${output}/owned-shared-v3.pentimento.json`);
  await uv('Create UV layer');assert.notDeepEqual(last.uv_layers_document,applied);
  await file();await page.getByRole('menuitem',{name:'Open...'}).click();await page.getByRole('button',{name:'Open',exact:true}).click();await settle();
  assert.deepEqual(last.uv_layers_document,applied);assert.equal(last.undo,0);assert.equal(last.redo,0);
  await canvas.focus();await page.keyboard.press('Shift+Tab');await settle();
  await uv('DirectUV surface');
  assert.equal(await modeControl.inputValue(),'Overlay');
  await uv('Detail');await page.getByLabel('Show UV layer Detail',{exact:true}).click();await settle();
  const pen=async(phase,x,pressure)=>{await canvas.dispatchEvent(`pointer${phase}`,{pointerType:'pen',pointerId:17,isPrimary:true,clientX:box.x+box.width*x,clientY:box.y+box.height/2,pressure});await settle();};
  const penBefore=last.uv_layers_document;
  await pen('down',.4,.25);await pen('move',.5,.5);await pen('up',.6,.7);
  const penAfter=last.uv_layers_document;assert.notDeepEqual(penAfter.layers,penBefore.layers);
  await uv('Undo UV layer edit');assert.deepEqual(last.uv_layers_document,penBefore);
  await uv('Redo UV layer edit');assert.deepEqual(last.uv_layers_document,penAfter);
  await page.screenshot({path:`${output}/uv-layers-reopened-pen.jpg`,type:'jpeg',quality:85});
  assert.deepEqual(errors,[]);
  const report={status:'passed',qualification:'actual Svelte controls -> native app dispatcher and native input forwarding -> production CPU UV and Canvas assets, owned v3 file I/O',native_cef_gpu:false,physical_stylus:false,
    checks:['create-name-select','DirectUV-selected-layer-mouse','selection-preserves-redo','visibility-opacity-lock','duplicate-both-reorder-directions-delete-recovery','Canvas-Apply-same-selected-layer','UV-Undo-separate-from-Canvas-source','v3-Save-Open-order-metadata-active-target','post-Open-DOM-pen-pressure-Undo-Redo','live-preview-repeated-strokes-and-source-Undo-Redo','live-owner-disables-stack-and-Direct','cancel-preview-retains-source-and-history','Apply-preview-once-pauses-live','blend-controls-CPU-upload-Undo-Redo-live-lock-v3-reopen','blend-opacity-order-raw-bits-CPU-image-Undo-Redo']};
  await writeFile(`${output}/browser-shared-uv-report.json`,`${JSON.stringify(report,null,2)}\n`);console.log(JSON.stringify(report));
} finally {
  await browser.close(); if(driver.exitCode===null)driver.stdin.end('{"stop":true}\n');
}
