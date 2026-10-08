// Read only receipts emitted after real backend work. DOM readiness and a Start
// event alone are deliberately insufficient for native framebuffer qualification.
import assert from 'node:assert/strict';

export function cefFramebufferReceipt(log) {
    if (!log.includes('CEF webview ready')) return null;
    const match = log.match(/First painted capture \(Cef mode\): (\d+)x(\d+), non-transparent pixels: (\d+)/);
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

// Observe production-owned history and the actual native framebuffer within one
// deadline. Never infer presentation from an accepted CPU transaction alone.
export async function waitForSculptPresentation(wait, { historyReady, capture, measure, description }) {
    return wait(async () => {
        if (!(await historyReady())) return null;
        const frame = await capture();
        const delta = measure(frame);
        return delta.changed_pixels > 25 ? { frame, delta } : null;
    }, `${description}: backend history and visible native deformation`, 30000);
}

// Four fresh parked-pointer updates span input acknowledgement, Last gizmo mesh
// changes, the next PostUpdate asset event flush, and pipelined render turnover.
// This is a scheduling barrier, not a substitute for native pixel assertions.
export function parkedPointerFrames(log, x, y) {
    let consecutive = 0;
    for (const match of log.matchAll(/Native capture after batch: last \(([-\d.]+), ([-\d.]+)\) blocked=(true|false) latched=(true|false) layout_received=(true|false)\r?\n/g)) {
        if (Number(match[1]) === x && Number(match[2]) === y && match[3] === 'false' && match[4] === 'false' && match[5] === 'true') consecutive++;
        else consecutive = 0;
    }
    return consecutive >= 4 ? { x, y, consecutive_updates: consecutive } : null;
}

// Preserve native key-down/up ordering, holding modifiers until the existing
// backend acknowledgement arrives. A deadline/error must always release keys.
export async function holdNativeKey(value, observed, description, { send, wait, pause, releaseFailed }) {
    let primaryError;
    try {
        send('keydown', value);
        await pause(200);
        if (observed) await wait(observed, description);
    } catch (error) {
        primaryError = error;
        throw error;
    } finally {
        try { send('keyup', value); } catch (error) {
            if (!primaryError) throw error;
            releaseFailed(error);
        }
    }
    await pause(350);
}

export function assertNativeClickBounds(rect, width, height) {
    assert.ok(rect && [rect.x, rect.y, rect.width, rect.height, width, height].every(Number.isFinite)
        && rect.width > 0 && rect.height > 0 && rect.x >= 0 && rect.y >= 0
        && rect.x + rect.width <= width && rect.y + rect.height <= height,
    `Native click target lies outside the viewport: ${JSON.stringify(rect)} in ${width}x${height}`);
}
