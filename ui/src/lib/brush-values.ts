/** Color inputs use sRGB; painting surfaces and IPC colors use linear RGB. */
export function colorToHex(color: number[]): string {
    return '#' + color.slice(0, 3).map(channel => {
        const linear = Math.max(0, Math.min(1, channel));
        const srgb = linear <= 0.0031308 ? linear * 12.92 : 1.055 * linear ** (1 / 2.4) - 0.055;
        return Math.round(srgb * 255).toString(16).padStart(2, '0');
    }).join('');
}

export function hexToColor(hex: string): [number, number, number, number] {
    const linear = (offset: number) => {
        const srgb = parseInt(hex.slice(offset, offset + 2), 16) / 255;
        return srgb <= 0.04045 ? srgb / 12.92 : ((srgb + 0.055) / 1.055) ** 2.4;
    };
    return [linear(1), linear(3), linear(5), 1];
}
