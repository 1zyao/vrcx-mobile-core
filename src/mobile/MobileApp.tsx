import { HashRouter, Navigate, Route, Routes } from 'react-router';

import { MOBILE_SCREENS } from '@/features/mobile/mobileRoutes';
import { MobileScreenStub } from '@/features/mobile/MobileScreenStub';
import { MobileShell } from '@/features/mobile/MobileShell';

/**
 * Standalone phone app.
 *
 * Routes mirror `MOBILE_SCREENS` (the VRCM 1.1.4 port checklist). There is no
 * desktop chrome in this tree at all — the shell is `MobileShell`, ported from
 * VRCM's `HomeScreen` scaffold.
 */
export function MobileApp() {
    return (
        <HashRouter>
            <Routes>
                <Route path="/" element={<MobileShell />}>
                    <Route index element={<Navigate to="/home" replace />} />
                    {MOBILE_SCREENS.map((screen) => (
                        <Route
                            key={screen.path}
                            path={screen.path}
                            element={<MobileScreenStub screen={screen} />}
                        />
                    ))}
                    <Route path="*" element={<Navigate to="/home" replace />} />
                </Route>
            </Routes>
        </HashRouter>
    );
}
