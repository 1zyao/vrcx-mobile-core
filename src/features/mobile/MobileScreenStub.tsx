import type { MobileScreen } from './mobileRoutes';

/**
 * Placeholder for a screen that has not been ported yet.
 *
 * It shows the route and the VRCM file it will be ported from — deliberately no
 * product copy, so nothing invented leaks into the UI while the port is
 * in progress.
 */
export function MobileScreenStub({ screen }: { screen: MobileScreen }) {
    return (
        <div className="flex h-full flex-col items-center justify-center gap-2 px-8 text-center">
            <code className="bg-muted text-muted-foreground rounded px-2 py-1 text-xs">
                /m/{screen.path}
            </code>
            <p className="text-muted-foreground text-xs">
                {screen.source}
            </p>
        </div>
    );
}
