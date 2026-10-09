// Real Svelte dialogs and IPC receipts; owned file operations are separately
// exercised through the actual shared Rust dispatcher, never browser mocks.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
const { chromium }=await import(process.env.PLAYWRIGHT_MODULE??'playwright-core');
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH??'/usr/bin/chromium',headless:true,args:['--no-sandbox']});
try{
 const page=await browser.newPage({viewport:{width:1280,height:900}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{window.commands=[];window.__PENTIMENTO_IPC__={postMessage:raw=>window.commands.push(JSON.parse(raw))};});
 await page.goto(process.env.PENTIMENTO_UI_URL??'http://127.0.0.1:5187');await page.waitForFunction(()=>window.commands.some(m=>m.type==='RequestBrushState'));
 const receive=m=>page.evaluate(m=>window.__PENTIMENTO_RECEIVE__(JSON.stringify(m)),m);
 const last=()=>page.evaluate(()=>window.commands.filter(m=>m.type==='ProjectCommand').at(-1)?.data);
 let project={path:null,available:true,active:false,blocked:false,notice:null};const state=()=>receive({type:'ProjectStateChanged',data:project});
 const file=()=>page.getByRole('button',{name:'File',exact:true}).click();
 await file();assert.equal(await page.getByRole('menuitem',{name:'Save',exact:true}).isDisabled(),true,'no claimed file capability before native state');await state();
 await page.getByRole('menuitem',{name:'Save',exact:true}).click();await page.getByRole('dialog',{name:'Save project'}).waitFor();
 await page.waitForFunction(()=>window.commands.filter(m=>m.type==='LayoutUpdate').at(-1)?.data.regions.some(r=>r.id==='project-dialog'&&r.x===0&&r.y===0&&r.width===1280&&r.height===900));
 const path='/tmp/ui-receipt-owned.pentimento.json';await page.getByLabel('Absolute local file path').fill(path);
 await page.keyboard.press('Shift+Tab');assert.equal(await page.evaluate(()=>document.activeElement?.textContent),'Save','reverse Tab stays in dialog');
 await page.keyboard.press('Tab');assert.equal(await page.getByLabel('Absolute local file path').evaluate(el=>el===document.activeElement),true);
 await page.getByRole('button',{name:'Save',exact:true}).click();assert.deepEqual(await last(),{Save:{path}});
 await page.keyboard.press('Tab');assert.equal(await page.getByRole('dialog').evaluate(el=>el===document.activeElement),true,'pending operation keeps keyboard in modal');
 await receive({type:'ProjectOperationFinished',data:{operation:'Save',success:false,message:'Project changed outside this editor.'}});assert.equal(await page.getByRole('dialog').count(),1);await page.getByRole('alert').filter({hasText:'Project changed outside this editor.'}).waitFor();
 await page.keyboard.press('Escape');assert.equal(await page.getByRole('dialog').count(),0);assert.equal(await page.getByRole('button',{name:'File',exact:true}).evaluate(el=>el===document.activeElement),true);
 await file();await page.getByRole('menuitem',{name:'Save',exact:true}).click();await page.getByLabel('Absolute local file path').fill(path);await page.getByRole('button',{name:'Save',exact:true}).click();
 project={...project,path,notice:'Project saved losslessly.'};await state();await receive({type:'ProjectOperationFinished',data:{operation:'Save',success:true,message:project.notice}});assert.equal(await page.getByRole('dialog').count(),0);await page.getByRole('status').filter({hasText:'Project saved losslessly.'}).waitFor();
 await page.waitForFunction(()=>window.commands.filter(m=>m.type==='LayoutUpdate').at(-1)?.data.regions.some(r=>r.id==='project-notice'&&r.width>0&&r.height>0));
 await file();await page.getByRole('menuitem',{name:'Save',exact:true}).click();assert.deepEqual(await last(),{Save:{path}});assert.equal(await page.getByRole('dialog').count(),0,'Save uses accepted local path');
 await file();project={...project,active:true};await state();assert.equal(await page.getByRole('menuitem',{name:'Open...'}).isDisabled(),true);assert.equal(await page.getByRole('menuitem',{name:'Save',exact:true}).isDisabled(),true);
 project={...project,active:false};await state();await receive({type:'EditModeChanged',data:{mode:'Sculpt'}});await page.getByRole('menuitem',{name:'Open...'}).click();await page.getByRole('dialog',{name:'Open project'}).waitFor();assert.equal(await page.getByLabel('Absolute local file path').inputValue(),path);
 await page.getByRole('button',{name:'Open',exact:true}).click();assert.deepEqual(await last(),{Open:{path}});await page.keyboard.press('Tab');assert.equal(await page.getByRole('dialog').evaluate(el=>el===document.activeElement),true);
 await receive({type:'EditModeChanged',data:{mode:'None'}});await receive({type:'ProjectionModeChanged',data:{live_projection:false}});await receive({type:'ProjectOperationFinished',data:{operation:'Open',success:true,message:'Project opened. Live projection is paused; undo history starts fresh.'}});
 assert.equal(await page.getByRole('dialog').count(),0);assert.equal(await page.getByLabel('Sculpt controls',{exact:true}).count(),0);await page.getByRole('status').filter({hasText:'Live projection is paused'}).waitFor();
 if(process.env.PENTIMENTO_PROJECT_UI_EVIDENCE){await mkdir(process.env.PENTIMENTO_PROJECT_UI_EVIDENCE,{recursive:true});await file();await page.getByRole('menuitem',{name:'Open...'}).click();await page.screenshot({path:`${process.env.PENTIMENTO_PROJECT_UI_EVIDENCE}/project-open-dialog.jpg`,type:'jpeg',quality:85});}
 assert.deepEqual(errors,[]);console.log(JSON.stringify({status:'passed',checks:['native-capability-gating','save-path-IPC','accepted-save-path-reuse','failed-save-keeps-dialog','active-stroke-controls-disabled','modal-Tab-Escape-focus-restoration','pending-modal-focus','open-IPC-and-authoritative-mode-reset','honest-paused-live-and-fresh-history-notice'],native_cef_acceptance:false,filesystem_qualification:'separate Rust owned-file tests'}));
}finally{await browser.close();}
