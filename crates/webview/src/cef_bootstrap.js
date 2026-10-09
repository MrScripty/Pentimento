// Executed by the canonical CEF host after first paint and on each UI receiver
// announcement. Safe to repeat after module remounts and page navigation.
(() => {
    if (!window.ipc) {
        window.ipc = {
            postMessage(message) { console.log('__PENTIMENTO_IPC__:' + message); }
        };
    }
    // Do not announce readiness before Svelte has installed its receiver.
    if (typeof window.__PENTIMENTO_RECEIVE__ === 'function') {
        window.dispatchEvent(new Event('pentimento:ipc-ready'));
    }
    window.ipc.postMessage(JSON.stringify({ type: 'UiDirty' }));
})();
