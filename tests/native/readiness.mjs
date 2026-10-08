// Read only receipts emitted after real backend work. DOM readiness and a Start
// event alone are deliberately insufficient for native framebuffer qualification.
import assert from 'node:assert/strict';

export function cefFramebufferReceipt(log) {
    if (!log.includes('CEF webview ready')) return null;
    const match = log.match(/First capture \(Cef mode\): (\d+)x(\d+), non-transparent pixels: (\d+)/);
    if (!match) return null;
    const [, width, height, painted] = match.map(Number);
    if (width <= 0 || height <= 0 || painted <= 0 || painted > width * height) return null;
    return { width, height, painted_pixels: painted };
}

export function startedStroke(log) {
    const match = log.match(/Sculpt stroke started: id=(\d+),/);
    return match ? { id: match[1] } : null;
}

export function completedStroke(log, id) {
    assert.match(String(id), /^\d+$/);
    const match = log.match(new RegExp(`Sculpt stroke completed: id=${id}, outcome=(accepted|no_change|rejected|cancelled), faces=(\\d+)`));
    return match ? { id: String(id), outcome: match[1], faces: Number(match[2]) } : null;
}

export function assertAcceptedStroke(completion, description) {
    assert.equal(completion.outcome, 'accepted', `${description}: transaction ${completion.id} must be accepted, got ${completion.outcome}`);
    return completion;
}

export function assertPaintedUiRegion(frame, width, height, rect, name) {
    assert.equal(frame.pixels.length, width * height * 3);
    assert.ok(rect && rect.width > 0 && rect.height > 0, `${name}: missing visible DOM rectangle`);
    const left = Math.max(0, Math.ceil(rect.x)), top = Math.max(0, Math.ceil(rect.y));
    const right = Math.min(width, Math.floor(rect.x + rect.width)), bottom = Math.min(height, Math.floor(rect.y + rect.height));
    let painted = 0, sampled = 0;
    for (let y = top; y < bottom; y++) for (let x = left; x < right; x++) {
        const i = (y * width + x) * 3;
        if (Math.max(frame.pixels[i], frame.pixels[i + 1], frame.pixels[i + 2]) > 15) painted++;
        sampled++;
    }
    // Existing panel backgrounds are opaque dark gray; sparse scene grid lines
    // behind an invisible CEF overlay must not masquerade as painted controls.
    assert.ok(sampled > 0 && painted / sampled > 0.15, `${name}: native framebuffer does not show the UI region (${painted}/${sampled} painted pixels)`);
    return { name, painted_pixels: painted, sampled_pixels: sampled };
}

// Qualification-only rendering choice, matching CEF's official software OSR
// sample. No sandbox, certificate, network, origin or other security switches.
export function cefRenderingArguments(mode) {
    if (mode === 'default') return [];
    if (mode === 'software') return ['--disable-gpu', '--disable-gpu-compositing'];
    throw new Error(`Unsupported CEF qualification rendering mode: ${mode}`);
}
