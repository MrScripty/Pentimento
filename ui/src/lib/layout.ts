import { bridge } from './bridge';

/** Report interactive rectangles to native hit-testing; cleanup is HMR-safe. */
export function setupInputLayout(root: HTMLElement): () => void {
    let frame = 0;
    let lastLayout = '';
    const refresh = () => {
        frame = 0;
        const regions = [...root.querySelectorAll<HTMLElement>('.toolbar, .side-panel, .brush-panel, .add-menu, .add-menu-backdrop, .dropdown, .global-error')].map((element, index) => {
            const rect = element.getBoundingClientRect();
            return { id: element.dataset.uiRegion ?? `ui-${index}`, x: rect.x, y: rect.y, width: rect.width, height: rect.height, z_index: 150, accepts_keyboard: true };
        }).filter(region => region.width > 0 && region.height > 0);
        const serialized = JSON.stringify(regions);
        if (serialized !== lastLayout) { lastLayout = serialized; bridge.updateLayout({ regions }); }
    };
    const schedule = () => { if (!frame) frame = requestAnimationFrame(refresh); };
    const focus = () => queueMicrotask(() => bridge.setUiInputCapture(root.contains(document.activeElement) && document.activeElement !== root));
    const blur = () => { bridge.setUiInputCapture(false); };
    const observer = new MutationObserver(schedule);
    observer.observe(root, { childList: true, subtree: true, attributes: true });
    window.addEventListener('resize', schedule);
    window.addEventListener('blur', blur);
    window.addEventListener('focus', focus);
    document.addEventListener('focusin', focus);
    document.addEventListener('focusout', focus);
    schedule();
    focus();
    return () => {
        observer.disconnect(); cancelAnimationFrame(frame);
        window.removeEventListener('resize', schedule); window.removeEventListener('blur', blur); window.removeEventListener('focus', focus);
        document.removeEventListener('focusin', focus); document.removeEventListener('focusout', focus);
    };
}
