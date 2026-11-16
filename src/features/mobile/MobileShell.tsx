import {
    MapPin,
    Newspaper,
    Search,
    Settings,
    Users,
    type LucideIcon
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { NavLink, Outlet, useLocation, useNavigate } from 'react-router';

import { cn } from '@/lib/utils';

type MobileTab = {
    to: string;
    /** Same keys the desktop route table uses for these page titles. */
    labelKey: string;
    icon: LucideIcon;
};

/**
 * Destinations and their order mirror the pager list VRCM 1.1.3 ships with:
 *
 *     FriendLocation → FriendList → Search → Feed
 *
 * The first three are VRCM's own; the feed is the single page VRCX adds (in
 * 1.1.3 it was a fourth pager). Nothing else is invented here.
 */
const MOBILE_TABS: MobileTab[] = [
    {
        to: '/m/locations',
        labelKey: 'app.routes.friend_locations',
        icon: MapPin
    },
    { to: '/m/friends', labelKey: 'app.routes.friends', icon: Users },
    { to: '/m/search', labelKey: 'app.routes.search', icon: Search },
    { to: '/m/feed', labelKey: 'app.routes.feed', icon: Newspaper }
];

function currentTitleKey(pathname: string) {
    if (pathname.startsWith('/m/me')) {
        return null;
    }
    return MOBILE_TABS.find((tab) => pathname.startsWith(tab.to))?.labelKey;
}

export function MobileShell() {
    const { t } = useTranslation();
    const { pathname } = useLocation();
    const navigate = useNavigate();
    const titleKey = currentTitleKey(pathname);

    return (
        <div className="bg-background flex h-full min-h-0 w-full flex-col overflow-hidden">
            {/* pt clears the system status bar. `env(safe-area-inset-top)` is 0
                inside Tauri's Android WebView (Android 15 forces edge-to-edge
                for targetSdk 35+ but the inset is not exposed to CSS), so a
                minimum is applied as well. */}
            <header className="bg-background/85 flex h-[calc(3rem+max(env(safe-area-inset-top),1.75rem))] shrink-0 items-end gap-2 px-4 pt-[max(env(safe-area-inset-top),1.75rem)] pb-2 backdrop-blur-sm">
                <h1 className="truncate text-[17px] leading-6 font-semibold tracking-tight">
                    {titleKey ? t(titleKey) : 'VRCX-0'}
                </h1>
                <button
                    type="button"
                    aria-label={t('app.routes.settings')}
                    onClick={() => navigate('/m/me')}
                    className="text-muted-foreground active:text-foreground -mr-2 ml-auto flex size-9 items-center justify-center rounded-full"
                >
                    <Settings className="size-[18px]" />
                </button>
            </header>

            <main className="min-h-0 flex-1 overflow-hidden">
                <Outlet />
            </main>

            {/*
             * Floating pill bar: inset from the screen edges and above the
             * system gesture area, so the content scrolls behind it the way
             * VRCM's glass tab bar does.
             */}
            <div className="pointer-events-none absolute inset-x-0 bottom-0 z-40">
                <div className="pointer-events-auto mx-4 mb-[max(0.875rem,env(safe-area-inset-bottom))] flex items-center">
                    <nav
                        aria-label="主导航"
                        className="border-border/50 bg-background/80 flex flex-1 items-center rounded-full border p-1.5 shadow-lg backdrop-blur-md"
                    >
                        {MOBILE_TABS.map((tab) => {
                            const Icon = tab.icon;
                            return (
                                <NavLink
                                    key={tab.to}
                                    to={tab.to}
                                    className={({ isActive }) =>
                                        cn(
                                            'flex h-10 flex-1 items-center justify-center gap-1.5 rounded-full text-[12px] transition-colors',
                                            isActive
                                                ? 'bg-foreground/8 text-foreground font-semibold'
                                                : 'text-muted-foreground'
                                        )
                                    }
                                >
                                    <Icon className="size-[18px]" />
                                    <span className="truncate">
                                        {t(tab.labelKey)}
                                    </span>
                                </NavLink>
                            );
                        })}
                    </nav>
                </div>
            </div>
        </div>
    );
}
