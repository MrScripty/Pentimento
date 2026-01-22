const { app, BrowserWindow } = require('electron');
console.log('app:', typeof app);
console.log('BrowserWindow:', typeof BrowserWindow);
app.whenReady().then(() => console.log('Ready!'));
