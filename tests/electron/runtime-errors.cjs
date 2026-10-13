// Bevy's browser tracing subscriber writes formatted ERROR records through
// console.info, so Chromium's console level alone is not a severity oracle.
function isFatalConsoleMessage(level, message) {
    return level === 'error'
        || /^%cERROR%c(?:\s|$)/.test(String(message))
        || /panicked at|unhandled rejection|WebGL.*context lost/i.test(String(message));
}

module.exports = { isFatalConsoleMessage };
