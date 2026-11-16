/**
 * Placeholder for mobile tabs that have no implementation yet.
 *
 * The vertical slice only ships the feed; the remaining tabs exist so the
 * navigation architecture can be exercised end to end instead of being
 * discovered later.
 */
export function MobilePlaceholderPage({
    title,
    note
}: {
    title: string;
    note: string;
}) {
    return (
        <div className="flex h-full flex-col items-center justify-center gap-2 px-8 text-center">
            <p className="text-base font-medium">{title}</p>
            <p className="text-muted-foreground text-sm">{note}</p>
        </div>
    );
}
