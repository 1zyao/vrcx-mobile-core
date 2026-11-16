import type { FeedRowOutput } from '@/platform/tauri/bindings';

export type MobileFeedTone =
    | 'online'
    | 'offline'
    | 'move'
    | 'status'
    | 'bio'
    | 'avatar'
    | 'unknown';

export type MobileFeedPresentation = {
    /** Short event label rendered as the card's chip. */
    label: string;
    /** Main line of the card. */
    primary: string;
    /** Optional supporting line. */
    secondary?: string;
    tone: MobileFeedTone;
};

function firstNonEmpty(...values: Array<string | null | undefined>) {
    for (const value of values) {
        const trimmed = value?.trim();
        if (trimmed) {
            return trimmed;
        }
    }
    return '';
}

/**
 * Maps a feed row onto the two lines a phone-sized card can show.
 *
 * The desktop table can display a dozen columns; a card cannot. Everything that
 * does not fit on `primary` / `secondary` is intentionally dropped rather than
 * truncated into noise.
 */
export function describeMobileFeedRow(
    row: FeedRowOutput
): MobileFeedPresentation {
    const status = firstNonEmpty(row.status, row.previousStatus);
    const location = firstNonEmpty(row.worldName, row.location);

    switch (row.type) {
        case 'Online':
            return {
                label: '上线',
                primary: status || '在线',
                secondary: firstNonEmpty(row.statusDescription) || undefined,
                tone: 'online'
            };
        case 'Offline':
            return {
                label: '下线',
                primary: '离线',
                secondary: location || undefined,
                tone: 'offline'
            };
        case 'GPS':
            return {
                label: '移动',
                primary: location || '未知位置',
                secondary: firstNonEmpty(row.groupName) || undefined,
                tone: 'move'
            };
        case 'Status':
            return {
                label: '状态',
                primary: status || '状态变更',
                secondary: firstNonEmpty(row.statusDescription) || undefined,
                tone: 'status'
            };
        case 'Bio':
            return {
                label: '简介',
                primary: firstNonEmpty(row.bio) || '简介变更',
                tone: 'bio'
            };
        case 'Avatar':
            // The row's owner id is a raw `usr_…` identifier; showing it on a
            // card is noise, and the avatar name is the only meaningful part.
            return {
                label: '换头像',
                primary: firstNonEmpty(row.avatarName) || '更换了头像',
                tone: 'avatar'
            };
        default:
            return {
                label: firstNonEmpty(row.type) || '事件',
                primary: location || status || '—',
                tone: 'unknown'
            };
    }
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

function pad(value: number) {
    return String(value).padStart(2, '0');
}

/**
 * Compact relative timestamp for cards ("3 分钟前", "昨天 21:04", "03-14 12:30").
 *
 * Accepts the raw `created_at` text from the read model. Timestamps without an
 * explicit zone are treated as local time, matching how the desktop renders
 * them; if the backend ever starts emitting UTC without a marker this will be
 * off by the local offset.
 */
export function formatMobileFeedTime(
    createdAt: string | null | undefined,
    now: number = Date.now()
): string {
    const trimmed = createdAt?.trim();
    if (!trimmed) {
        return '';
    }
    const parsed = new Date(trimmed.includes('T') ? trimmed : trimmed.replace(' ', 'T'));
    const time = parsed.getTime();
    if (Number.isNaN(time)) {
        return trimmed;
    }

    const delta = now - time;
    if (delta < MINUTE) {
        return '刚刚';
    }
    if (delta < HOUR) {
        return `${Math.floor(delta / MINUTE)} 分钟前`;
    }
    if (delta < DAY) {
        return `${Math.floor(delta / HOUR)} 小时前`;
    }

    const clock = `${pad(parsed.getHours())}:${pad(parsed.getMinutes())}`;
    if (delta < 2 * DAY) {
        return `昨天 ${clock}`;
    }
    return `${pad(parsed.getMonth() + 1)}-${pad(parsed.getDate())} ${clock}`;
}
