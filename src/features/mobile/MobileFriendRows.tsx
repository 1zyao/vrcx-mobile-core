import { cn } from '@/lib/utils';
import { userImage } from '@/services/entityMediaService';
import type { SidebarVirtualRow } from '@/components/sidebar/friends-sidebar/friendsSidebarVirtualRowBuilder';

/**
 * Presentational rows for the mobile friend board.
 *
 * Ported from VRCM 1.1.4's home list (`screens/home/HomeScreen.kt`) but fed by
 * VRCX-0's own row model: everything here comes from a `SidebarVirtualRow`, so
 * the grouping, ordering and section titles are the ones the desktop right
 * sidebar already computes.
 *
 * Only the pieces the row model actually carries are rendered — no field is
 * guessed. Avatars resolve through `userImage`, which returns a plain HTTPS
 * `/image/<fileId>/<version>/<resolution>` URL.
 */

const AVATAR_SIZE = 44;

function avatarHue(seed: string) {
    let hash = 0;
    for (let index = 0; index < seed.length; index += 1) {
        hash = (hash * 31 + seed.charCodeAt(index)) | 0;
    }
    return Math.abs(hash) % 360;
}

export function MobileSectionHeader({ row }: { row: SidebarVirtualRow }) {
    return (
        <div className="flex items-center justify-between px-1 pt-3 pb-1.5">
            <span className="text-muted-foreground text-[13px] font-medium">
                {row.title}
            </span>
            {typeof row.count === 'number' && row.count > 0 ? (
                <span className="text-muted-foreground text-[13px] tabular-nums">
                    {row.count}
                </span>
            ) : null}
        </div>
    );
}

export function MobileFriendRow({
    row,
    onOpen
}: {
    row: SidebarVirtualRow;
    onOpen?: () => void;
}) {
    const friend = row.friend;
    const displayName = friend?.displayName?.trim() || friend?.id || '—';
    const image = userImage(friend, AVATAR_SIZE * 2);
    const hue = avatarHue(friend?.id ?? displayName);
    const location = row.location?.trim();

    return (
        <button
            type="button"
            onClick={onOpen}
            className="active:bg-foreground/5 flex w-full items-center gap-3 px-4 py-2.5 text-left"
        >
            {image ? (
                <img
                    src={image}
                    alt=""
                    loading="lazy"
                    className="bg-muted size-11 shrink-0 rounded-full object-cover"
                />
            ) : (
                <span
                    style={{
                        backgroundColor: `hsl(${hue} 55% 88%)`,
                        color: `hsl(${hue} 45% 30%)`
                    }}
                    className="flex size-11 shrink-0 items-center justify-center rounded-full text-[15px] font-semibold"
                >
                    {displayName.slice(0, 1).toUpperCase()}
                </span>
            )}
            <span className="min-w-0 flex-1">
                <span
                    className={cn('block truncate text-[15px] font-medium')}
                    style={
                        friend?.$userColour
                            ? { color: friend.$userColour }
                            : undefined
                    }
                >
                    {displayName}
                </span>
                {location ? (
                    <span className="text-muted-foreground mt-0.5 block truncate text-xs">
                        {location}
                    </span>
                ) : null}
            </span>
        </button>
    );
}
