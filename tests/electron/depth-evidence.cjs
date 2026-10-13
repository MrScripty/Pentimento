const assert = require('node:assert/strict');

function inspectUnavailableDepth(state) {
    assert.equal(state.disabled, true, 'GL depth control must be disabled');
    assert.equal(state.pressed, 'false', 'GL depth control must not report enabled');
    assert.match(state.title, /unavailable.*OpenGL\/WebGL/i, 'depth capability must resolve to the actual GL limitation');
    assert.equal(state.status, state.title, 'accessible depth status must explain the same limitation');
    assert.equal(state.describedBy, 'depth-view-status');
    assert.equal(state.statusHidden, false, 'depth unavailability must be visible');
    return state;
}

module.exports = { inspectUnavailableDepth };
