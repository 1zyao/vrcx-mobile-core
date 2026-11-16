import { createElement, StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import '@/styles/globals.css';

/**
 * Entry point for the phone build.
 *
 * The mobile frontend is a port of VRCM 1.1.4's page set and shares nothing with
 * the desktop shell: no `src/app/router.tsx`, no layout/sidebar/titlebar, no
 * desktop pages. What it does share is the data layer — every page reads through
 * VRCX-0 repositories, which call the generated commands in
 * `platform/tauri/bindings.ts`.
 */
async function bootstrap() {
    // Imported for its side effect: initialises the shared i18next instance.
    const [, { MobileApp }] = await Promise.all([
        import('@/services/i18nService'),
        import('./MobileApp')
    ]);

    const rootElement = document.getElementById('root');
    if (!rootElement) {
        throw new Error('Missing #root mount node');
    }

    createRoot(rootElement).render(
        createElement(StrictMode, null, createElement(MobileApp))
    );
}

bootstrap().catch((error: unknown) => {
    console.error(error);
});
