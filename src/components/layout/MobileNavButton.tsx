import { Menu } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { cn } from '@/lib/utils';
import { useSidebar } from '@/ui/shadcn/sidebar';

/**
 * Phone-only entry point for the navigation drawer.
 *
 * shadcn's `Sidebar` already renders the navigation as a `Sheet` below the
 * mobile breakpoint (`src/lib/use-mobile.ts`), but nothing in the app ever
 * called `toggleSidebar`, so on a phone the drawer could not be opened at all
 * and the app had no visible navigation. This is that missing trigger.
 */
export function MobileNavButton({ className }: { className?: string }) {
    const { t } = useTranslation();
    const { isMobile, openMobile, toggleSidebar } = useSidebar();

    if (!isMobile) {
        return null;
    }

    return (
        <button
            type="button"
            aria-label={t('accessibility.sidebar')}
            aria-expanded={openMobile}
            onClick={toggleSidebar}
            className={cn(
                'bg-background/90 text-foreground border-border fixed bottom-14 left-3 z-50 flex size-11 items-center justify-center rounded-full border shadow-lg backdrop-blur',
                className
            )}
        >
            <Menu className="size-5" />
        </button>
    );
}
