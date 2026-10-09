import { bridge } from './bridge';

/** Report interactive rectangles to native hit-testing; cleanup is HMR-safe. */
export function setupInputLayout(root: HTMLElement): () => void {
    let frame = 0;
    let fallback: ReturnType<typeof setTimeout> | null = null;
    let disposed = false;
    let lastLayout = '';
    const refresh = () => {
        if (disposed) return;
        cancelAnimationFrame(frame);
        frame = 0;
        if (fallback !== null) clearTimeout(fallback);
        fallback = null;
        const regions = [...root.querySelectorAll<HTMLElement>('.toolbar, .side-panel, .brush-panel, .add-menu, .add-menu-backdrop, .dropdown, .global-error, .project-backdrop, .global-project-notice')].map((element, index) => {
            const rect = element.getBoundingClientRect();
            return { id: element.dataset.uiRegion ?? `ui-${index}`, x: rect.x, y: rect.y, width: rect.width, height: rect.height, z_index: 150, accepts_keyboard: true };
        }).filter(region => region.width > 0 && region.height > 0);
        const serialized = JSON.stringify(regions);
        if (serialized !== lastLayout) { lastLayout = serialized; bridge.updateLayout({ regions }); }
    };
    // Offscreen native browsers may not produce animation frames promptly.
    // Either callback drains the same job and cancels its counterpart.
    const schedule = () => {
        if (disposed || fallback !== null) return;
        frame = requestAnimationFrame(refresh);
        fallback = setTimeout(refresh, 16);
    };
    const focus = () => queueMicrotask(() => {
        if (!disposed) bridge.setUiInputCapture(root.contains(document.activeElement) && document.activeElement !== root);
    });
    const blur = () => { bridge.setUiInputCapture(false); };
    const observer = new MutationObserver(schedule);
    observer.observe(root, { childList: true, subtree: true, attributes: true });
    window.addEventListener('resize', schedule);
    window.addEventListener('blur', blur);
    window.addEventListener('focus', focus);
    document.addEventListener('focusin', focus);
    document.addEventListener('focusout', focus);
    refresh();
    focus();
    return () => {
        disposed = true;
        observer.disconnect(); cancelAnimationFrame(frame);
        if (fallback !== null) clearTimeout(fallback);
        window.removeEventListener('resize', schedule); window.removeEventListener('blur', blur); window.removeEventListener('focus', focus);
        document.removeEventListener('focusin', focus); document.removeEventListener('focusout', focus);
    };
}
