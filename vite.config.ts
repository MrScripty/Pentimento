import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import path from 'path';

export default defineConfig({
    plugins: [
        svelte(),
        tailwindcss(),
    ],
    root: './ui',
    publicDir: '../dist/wasm-public',
    // Use relative paths for assets (required for Electron file:// protocol)
    base: './',
    build: {
        outDir: '../dist/ui',
        emptyOutDir: true,
        rollupOptions: {
            output: {
                entryFileNames: 'assets/[name].js',
                chunkFileNames: 'assets/[name].js',
                assetFileNames: 'assets/[name].[ext]',
            },
        },
    },
    resolve: {
        alias: {
            '$lib': path.resolve('./ui/src/lib'),
        },
    },
    server: {
        port: 5173,
        strictPort: true,
        fs: {
            // Allow serving files from dist/wasm during development
            allow: ['..'],
        },
    },
});
