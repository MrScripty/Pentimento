interface Point { x: number; y: number }
interface Size { width: number; height: number }

/** Keep a measured floating menu inside the viewport, with the panel gutter. */
export function clampMenuPosition(position: Point, menu: Size, viewport: Size): Point {
    const gutter = 8;
    const clamp = (requested: number, extent: number, available: number) => {
        const max = Math.max(0, available - extent);
        const min = Math.min(gutter, max);
        return Math.min(Math.max(requested, min), Math.max(min, max - gutter));
    };
    return {
        x: clamp(position.x, menu.width, viewport.width),
        y: clamp(position.y, menu.height, viewport.height),
    };
}
