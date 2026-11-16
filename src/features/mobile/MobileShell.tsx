import { Bell, Heart, House, Search } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { NavLink, Outlet, useLocation, useNavigate } from 'react-router';

import { cn } from '@/lib/utils';

/**
 * Phone shell, laid out after VRCM 1.1.4's home scaffold: a floating pill with
 * three destinations plus a separate circular search button
 * (`presentation/screens/home/HomeScreen.kt`, `MainNavigationBar`).
 *
 * The screens themselves are ported one at a time; see `mobileRoutes.ts`.
 */
const NAV_ITEMS = [
    { to: '/home', icon: House, key: 'view.home.title', fallback: 'Home' },
    {
        to: '/favorites',
        icon: Heart,
        key: 'view.favorites.title',
        fallback: 'Favorites'
    },
    {
        to: '/inbox',
        icon: Bell,
        key: 'view.notification.title',
        fallback: 'Inbox'
    }
] as const;

export function MobileShell() {
    const { t } = useTranslation();
    const { pathname } = useLocation();
    const navigate = useNavigate();

    return (
        <div className="bg-muted/40 flex h-full min-h-0 w-full flex-col overflow-hidden">
            <main className="min-h-0 flex-1 overflow-hidden">
                <Outlet />
            </main>

            <div className="pointer-events-none absolute inset-x-0 bottom-0 z-40">
                <div className="pointer-events-auto mx-4 mb-[max(0.875rem,env(safe-area-inset-bottom))] flex items-center gap-2.5">
                    <nav
                        aria-label={t('accessibility.sidebar', {
                            defaultValue: 'Navigation'
                        })}
                        className="border-border/50 bg-background/80 flex flex-1 items-center rounded-full border p-1.5 shadow-lg backdrop-blur-md"
                    >
                        {NAV_ITEMS.map((item) => {
                            const Icon = item.icon;
                            return (
                                <NavLink
                                    key={item.to}
                                    to={item.to}
                                    className={({ isActive }) =>
                                        cn(
                                            'flex h-11 flex-1 flex-col items-center justify-center gap-0.5 rounded-full text-[11px] transition-colors',
                                            isActive
                                                ? 'bg-foreground/8 text-foreground font-semibold'
                                                : 'text-muted-foreground'
                                        )
                                    }
                                >
                                    <Icon className="size-[20px]" />
                                    <span className="truncate">
                                        {t(item.key, {
                                            defaultValue: item.fallback
                                        })}
                                    </span>
                                </NavLink>
                            );
                        })}
                    </nav>
                    <button
                        type="button"
                        aria-label={t('view.search.title', {
                            defaultValue: 'Search'
                        })}
                        onClick={() => navigate('/search')}
                        className={cn(
                            'border-border/50 bg-background/80 flex size-[3.5rem] shrink-0 items-center justify-center rounded-full border shadow-lg backdrop-blur-md',
                            pathname.startsWith('/search')
                                ? 'text-foreground'
                                : 'text-muted-foreground'
                        )}
                    >
                        <Search className="size-[20px]" />
                    </button>
                </div>
            </div>
        </div>
    );
}

