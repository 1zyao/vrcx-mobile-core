import { resolve } from 'node:path';

import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

/**
 * Build for the standalone phone app (`mobile.html`).
 *
 * Kept separate from the desktop config so the two frontends never share a
 * bundle: the phone build contains only the VRCM-derived pages under src/mobile
 * plus the data layer they import.
 */
export default defineConfig({
    base: '',
    plugins: [react(), tailwindcss()],
    resolve: {
        alias: {
            '@': resolve(import.meta.dirname, 'src')
        }
    },
    build: {
        outDir: 'dist-mobile',
        emptyOutDir: true,
        rollupOptions: {
            input: resolve(import.meta.dirname, 'mobile.html')
        }
    }
});
