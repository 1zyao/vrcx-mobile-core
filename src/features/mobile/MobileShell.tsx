import {
    Bell,
    Newspaper,
    Search,
    Settings,
    Users,
    type LucideIcon
} from 'lucide-react';
import { NavLink, Outlet, useLocation, useNavigate } from 'react-router';

import { cn } from '@/lib/utils';

type MobileTab = {
    to: string;
    label: string;
    icon: LucideIcon;
};

/**
 * Primary destinations, following the layout VRCM 1.1.4 converged on for phones:
 * three destinations in a floating pill bar plus a separate circular search
 * button. Settings and the account live behind the top bar, not in a fourth tab.
 */
const MOBILE_TABS: MobileTab[] = [
    { to: '/m/feed', label: '动态', icon: Newspaper },
    { to: '/m/friends', label: '好友', icon: Users },
    { to: '/m/notifications', label: '通知', icon: Bell }
];

function currentTitle(pathname: string) {
    if (pathname.startsWith('/m/search')) {
        return '搜索';
    }
    if (pathname.startsWith('/m/me')) {
        return '我的';
    }
    const match = MOBILE_TABS.find((tab) => pathname.startsWith(tab.to));
    return match?.label ?? 'VRCX-0';
}

export function MobileShell() {
    const { pathname } = useLocation();
    const navigate = useNavigate();

    return (
        <div className="bg-background flex h-full min-h-0 w-full flex-col overflow-hidden">
            {/* pt clears the system status bar. `env(safe-area-inset-top)` is 0
                inside Tauri's Android WebView (Android 15 forces edge-to-edge
                for targetSdk 35+ but the inset is not exposed to CSS), so a
                minimum is applied as well. */}
            <header className="border-border/60 flex h-[calc(3rem+max(env(safe-area-inset-top),1.75rem))] shrink-0 items-end gap-2 border-b px-4 pt-[max(env(safe-area-inset-top),1.75rem)] pb-2">
                <h1 className="truncate text-sm font-semibold">
                    {currentTitle(pathname)}
                </h1>
                <button
                    type="button"
                    aria-label="我的"
                    onClick={() => navigate('/m/me')}
                    className="text-muted-foreground hover:text-foreground active:text-foreground ml-auto flex size-9 items-center justify-center rounded-full"
                >
                    <Settings className="size-5" />
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
            <div className="pointer-events-none relative z-40 shrink-0">
                <div className="pointer-events-auto mx-4 mb-[max(0.75rem,env(safe-area-inset-bottom))] flex items-center gap-2.5">
                    <nav
                        aria-label="主导航"
                        className="border-border/60 bg-background/85 flex flex-1 items-center gap-1 rounded-full border p-1 shadow-lg backdrop-blur"
                    >
                        {MOBILE_TABS.map((tab) => {
                            const Icon = tab.icon;
                            return (
                                <NavLink
                                    key={tab.to}
                                    to={tab.to}
                                    className={({ isActive }) =>
                                        cn(
                                            'flex h-11 flex-1 items-center justify-center gap-1.5 rounded-full text-xs transition-colors',
                                            isActive
                                                ? 'bg-foreground/10 text-foreground font-medium'
                                                : 'text-muted-foreground'
                                        )
                                    }
                                >
                                    <Icon className="size-5" />
                                    <span className="truncate">
                                        {tab.label}
                                    </span>
                                </NavLink>
                            );
                        })}
                    </nav>
                    <NavLink
                        to="/m/search"
                        aria-label="搜索"
                        className={({ isActive }) =>
                            cn(
                                'border-border/60 bg-background/85 flex size-12 shrink-0 items-center justify-center rounded-full border shadow-lg backdrop-blur',
                                isActive
                                    ? 'text-foreground'
                                    : 'text-muted-foreground'
                            )
                        }
                    >
                        <Search className="size-5" />
                    </NavLink>
                </div>
            </div>
        </div>
    );
}
