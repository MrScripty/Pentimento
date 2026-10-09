// Inspect installed files without requiring Electron, which can download on import.
const fs = require('node:fs');
const path = require('node:path');

function checkInstallation(root, platform = process.platform) {
    try {
        const binaryNames = {
            linux: 'electron',
            darwin: 'Electron.app/Contents/MacOS/Electron',
            win32: 'electron.exe',
        };
        const binaryName = binaryNames[platform];
        if (!binaryName) return false;
        const readJson = (file) => JSON.parse(fs.readFileSync(path.join(root, file), 'utf8'));
        const manifest = readJson('package.json');
        const lock = readJson('package-lock.json');
        const installed = readJson('node_modules/electron/package.json');
        const expected = lock.packages['node_modules/electron'].version;
        if (lock.packages[''].devDependencies.electron !== manifest.devDependencies.electron) return false;
        if (installed.version !== expected) return false;
        const electronRoot = path.join(root, 'node_modules/electron');
        const version = fs.readFileSync(path.join(electronRoot, 'dist/version'), 'utf8').trim().replace(/^v/, '');
        const binaryPath = fs.readFileSync(path.join(electronRoot, 'path.txt'), 'utf8').trim();
        if (version !== expected || binaryPath !== binaryName) return false;
        fs.accessSync(path.join(root, 'node_modules/.bin/electron'), fs.constants.X_OK);
        const executable = path.join(electronRoot, 'dist', binaryName);
        if (!fs.statSync(executable).isFile()) return false;
        fs.accessSync(executable, fs.constants.X_OK);
        return true;
    } catch {
        return false;
    }
}

module.exports = { checkInstallation };
if (require.main === module) {
    process.exitCode = checkInstallation(__dirname) ? 0 : 1;
}
