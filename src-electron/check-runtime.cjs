const fs = require('node:fs');
const path = require('node:path');

// Electron's package entrypoint downloads a missing binary. Preflight must only
// inspect the pinned installation; provisioning belongs to install:runtime.
function checkRuntime(shellDirectory = __dirname) {
    if (process.platform !== 'linux' || process.arch !== 'x64') {
        throw new Error('Only Linux x86_64 is supported by the Electron launcher.');
    }

    const manifest = JSON.parse(fs.readFileSync(path.join(shellDirectory, 'package.json'), 'utf8'));
    const runtimeDirectory = path.join(shellDirectory, 'node_modules', 'electron');
    const installed = JSON.parse(fs.readFileSync(path.join(runtimeDirectory, 'package.json'), 'utf8'));
    const expectedVersion = manifest.devDependencies.electron;
    if (installed.version !== expectedVersion) {
        throw new Error(`Electron npm package ${installed.version} does not match pinned ${expectedVersion}.`);
    }

    const version = fs.readFileSync(path.join(runtimeDirectory, 'dist', 'version'), 'utf8').trim().replace(/^v/, '');
    if (version !== expectedVersion) {
        throw new Error(`Electron binary ${version} does not match pinned ${expectedVersion}.`);
    }
    const executableName = fs.readFileSync(path.join(runtimeDirectory, 'path.txt'), 'utf8').trim();
    if (executableName !== 'electron') {
        throw new Error('Electron binary path does not match the supported Linux installation.');
    }

    const executable = path.join(runtimeDirectory, 'dist', executableName);
    if (!fs.statSync(executable).isFile()) {
        throw new Error('Electron runtime executable is not a file.');
    }
    fs.accessSync(executable, fs.constants.X_OK);
    return executable;
}

if (require.main === module) {
    try {
        console.log(checkRuntime());
    } catch (error) {
        console.error(`[electron] ${error.message} Run ./launcher.sh --install.`);
        process.exitCode = 1;
    }
}

module.exports = { checkRuntime };
