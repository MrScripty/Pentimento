// Expose Electron detection to renderer
// This runs in the preload context with access to both Node.js and DOM

declare global {
    interface Window {
        __ELECTRON__: boolean;
    }
}

window.__ELECTRON__ = true;

export {};
