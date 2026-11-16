import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';

import type { SidebarVirtualRow } from '@/components/sidebar/friends-sidebar/friendsSidebarVirtualRowBuilder';
import type { SidebarFriendRecord } from '@/components/sidebar/friends-sidebar/friendsSidebarModel';
import { presenceOf, presenceSection } from '@/domain/friends/presence';
import { useFriendRosterStore } from '@/state/friendRosterStore';

import { MobileFriendRow, MobileSectionHeader } from './MobileFriendRows';

/**
 * Mobile friend board — the port of VRCM 1.1.4's home screen
 * (`screens/home/HomeScreen.kt`), fed by VRCX-0's friend roster store.
 *
 * This first cut renders the three presence sections only (在线 / 活跃中 / 离线).
 * Same-room grouping and favourites need the extra sidebar inputs and are next.
 */
const SECTIONS = [
    { bucket: 'online', labelKey: 'side_panel.online' },
    { bucket: 'active', labelKey: 'side_panel.active' },
    { bucket: 'offline', labelKey: 'side_panel.offline' }
] as const;

export function MobileHomePage() {
    const { t } = useTranslation();
    const friendsById = useFriendRosterStore((state) => state.friendsById);

    const rows = useMemo<SidebarVirtualRow[]>(() => {
        const buckets = new Map<string, SidebarFriendRecord[]>(
            SECTIONS.map((section) => [section.bucket, []])
        );

        for (const entry of Object.values(friendsById ?? {})) {
            const view = presenceOf(entry);
            if (!view) {
                continue;
            }
            const bucket = presenceSection(view);
            buckets.get(bucket)?.push(entry as unknown as SidebarFriendRecord);
        }

        const result: SidebarVirtualRow[] = [];
        for (const section of SECTIONS) {
            const friends = buckets.get(section.bucket) ?? [];
            if (friends.length === 0) {
                continue;
            }
            result.push({
                type: 'section',
                key: `section:${section.bucket}`,
                title: t(section.labelKey),
                count: friends.length
            });
            for (const friend of friends) {
                result.push({
                    type: 'friend',
                    key: `friend:${section.bucket}:${friend.id}`,
                    friend
                });
            }
        }
        return result;
    }, [friendsById, t]);

    const total = rows.filter((row) => row.type === 'friend').length;

    return (
        <div className="bg-muted/40 flex h-full min-h-0 flex-col">
            <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 pt-2 pb-24">
                {total === 0 ? (
                    <p className="text-muted-foreground p-6 text-center text-sm">
                        {t('common.loading', { defaultValue: '…' })}
                    </p>
                ) : (
                    <div className="bg-card overflow-hidden rounded-xl shadow-sm">
                        {rows.map((row, index) =>
                            row.type === 'section' ? (
                                <MobileSectionHeader key={row.key} row={row} />
                            ) : (
                                <div key={row.key}>
                                    {index > 0 &&
                                    rows[index - 1]?.type === 'friend' ? (
                                        <div className="bg-border/50 ml-[4.5rem] h-px" />
                                    ) : null}
                                    <MobileFriendRow row={row} />
                                </div>
                            )
                        )}
                    </div>
                )}
            </div>
        </div>
    );
}
