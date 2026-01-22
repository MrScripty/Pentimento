// Expose Electron detection to renderer
// With contextIsolation: true, we need to use contextBridge or inject via executeJavaScript
// For simplicity, we inject a script tag that sets the global before other scripts run

const { contextBridge } = require('electron');

// Expose __ELECTRON__ flag via contextBridge
contextBridge.exposeInMainWorld('__ELECTRON__', true);
