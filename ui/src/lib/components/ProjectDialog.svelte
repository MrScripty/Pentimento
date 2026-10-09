<script lang="ts">
    import { onMount,tick } from 'svelte';
    import { bridge } from '$lib/bridge';
    import type { ProjectState } from '$lib/types';
    let { operation, project, onclose }: { operation:'Save'|'Open'|'New';project:ProjectState;onclose:()=>void } = $props();
    // Freeze the owner at confirmation creation; later state receipts cannot retarget it.
    let generation="";
    let path=$state('');
    let pending=$state(false);
    let error=$state<string|null>(null);
    let input=$state<HTMLInputElement>();
    let dialog:HTMLDivElement;
    onMount(()=>{
        const previous=document.activeElement;
        generation=project.generation;
        path=project.path ?? '';
        if(operation==='New')dialog.querySelector<HTMLButtonElement>('button')?.focus();else input?.focus();
        const unsubscribe=bridge.subscribe(message=>{
            if(message.type==='ProjectOperationFinished' && message.data.operation===operation){
                pending=false;
                if(message.data.success) onclose(); else error=message.data.message;
            }
        });
        return ()=>{unsubscribe();if(previous instanceof HTMLElement && previous.isConnected && previous!==document.body && previous!==document.documentElement)previous.focus();else document.querySelector<HTMLButtonElement>('.toolbar .nav-button')?.focus();};
    });
    function keyboard(event:KeyboardEvent){
        event.stopPropagation();
        if(event.key==='Escape'){event.preventDefault();if(!pending)onclose();}
        if(event.key==='Tab'){
            const controls=Array.from(dialog.querySelectorAll<HTMLElement>('input:not(:disabled),button:not(:disabled)'));
            if(!controls.length){event.preventDefault();dialog.focus();return;}
            const first=controls[0],last=controls.at(-1);
            if(event.shiftKey&&document.activeElement===first){event.preventDefault();last?.focus();}
            else if(!event.shiftKey&&document.activeElement===last){event.preventDefault();first?.focus();}
        }
    }
    function submit(event:SubmitEvent){
        event.preventDefault();
        if(pending || project.active || !project.available || (operation!=='New' && !path.trim()))return;
        pending=true;error=null;
        void tick().then(()=>{if(pending)dialog.focus();});
        bridge.projectCommand(operation==='New'?{New:{expected_generation:generation,confirm_discard:true}}:operation==='Save'?{Save:{path:path.trim()}}:{Open:{path:path.trim()}});
    }
</script>
<div class="project-backdrop" data-ui-region="project-dialog">
    <div bind:this={dialog} class="project-dialog" role="dialog" aria-modal="true" aria-labelledby="project-title" tabindex="-1" onkeydown={keyboard}>
        <h2 id="project-title">{operation === 'New' ? 'New project' : operation === 'Save' ? 'Save project' : 'Open project'}</h2>
        <p>{operation === 'New' ? 'Create an empty project? Any unsaved changes and all local Undo/Redo history will be discarded. Existing project files will remain on disk.' : operation === 'Save' ? 'Save editable geometry and paint layers to a local .pentimento.json file.' : 'Open a local .pentimento.json file and replace this document. Undo history starts fresh; live projection opens paused.'}</p>
        <form onsubmit={submit}>
            {#if operation!=='New'}<label for="project-path">Absolute local file path</label>
            <input id="project-path" bind:this={input} bind:value={path} placeholder="/home/me/artwork.pentimento.json" required disabled={pending} />{/if}
            {#if error}<p role="alert" class="error">{error}</p>{/if}
            <div class="actions">
                <button type="button" onclick={onclose} disabled={pending}>Cancel</button>
                <button type="submit" disabled={pending || project.active || !project.available || (operation!=='New' && !path.trim())}>{pending ? 'Working…' : operation==='New'?'Discard and create new':operation}</button>
            </div>
        </form>
    </div>
</div>
<style>
    .project-backdrop{position:fixed;inset:0;z-index:200;background:#0009;display:flex;align-items:center;justify-content:center}
    .project-dialog{width:min(540px,90vw);padding:24px;border:1px solid #555;border-radius:8px;background:#25262a;box-shadow:0 12px 40px #0008;color:#eee}
    h2{font-size:20px;margin:0 0 12px}p{font-size:13px;line-height:1.5;margin:0 0 16px;color:#ccc}
    label{display:block;font-size:13px;margin-bottom:6px}input{box-sizing:border-box;width:100%;padding:10px;border:1px solid #666;border-radius:4px;background:#16171a;color:#fff}
    .actions{display:flex;justify-content:flex-end;gap:10px;margin-top:20px}button{padding:8px 16px;border:1px solid #666;border-radius:4px;background:#383b43;color:#fff}button:disabled{opacity:.5}.error{color:#ffb0a8;margin-top:14px}
</style>
