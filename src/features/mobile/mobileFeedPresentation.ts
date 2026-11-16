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
    /**
     * Feed event type. The label is translated by the caller through the same
     * key the desktop table uses: `view.feed.filters.<type>`.
     */
    typeKey: string;
    /**
     * Translation key for a fallback line, used when the row carries no
     * human-readable text of its own (uncached world, etc).
     */
    primaryKey?: string;
    /** Row-derived text: names, statuses and bios are shown verbatim. */
    primary?: string;
    /** Optional supporting line. */
    secondary?: string;
    tone: MobileFeedTone;
};

const UNKNOWN_WORLD_KEY = 'dashboard.widget.unknown_world';

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
 * `location` falls back to either a raw VRChat identifier (`wrld_…:instance`,
 * `grp_…`) or a travel token such as `private`/`offline` whenever the world is
 * not in the local cache. Neither belongs on a card: ids are unreadable and the
 * tokens have no translation key, so they are dropped and the caller shows the
 * translated stand-in instead.
 */
const LOCATION_TOKENS = new Set(['private', 'offline', 'traveling', '']);

function readableLocation(value: string | null | undefined) {
    const trimmed = value?.trim() ?? '';
    if (!trimmed) {
        return '';
    }
    if (
        trimmed.startsWith('wrld_') ||
        trimmed.startsWith('grp_') ||
        LOCATION_TOKENS.has(trimmed.toLowerCase())
    ) {
        return '';
    }
    return trimmed;
}

function readableGroupName(value: string | null | undefined) {
    const trimmed = value?.trim() ?? '';
    if (!trimmed || trimmed.startsWith('grp_') || trimmed.startsWith('wrld_')) {
        return '';
    }
    return trimmed;
}

export function describeMobileFeedRow(
    row: FeedRowOutput
): MobileFeedPresentation {
    const status = firstNonEmpty(row.status, row.previousStatus);
    const location = readableLocation(
        firstNonEmpty(row.worldName, row.location)
    );
    const typeKey = firstNonEmpty(row.type) || 'GPS';

    switch (row.type) {
        case 'Online':
            return {
                typeKey,
                primary: status,
                secondary: firstNonEmpty(row.statusDescription) || undefined,
                tone: 'online'
            };
        case 'Offline':
            return {
                typeKey,
                primary: location,
                primaryKey: location ? undefined : UNKNOWN_WORLD_KEY,
                tone: 'offline'
            };
        case 'GPS':
            return {
                typeKey,
                primary: location,
                primaryKey: location ? undefined : UNKNOWN_WORLD_KEY,
                secondary: readableGroupName(row.groupName) || undefined,
                tone: 'move'
            };
        case 'Status':
            return {
                typeKey,
                primary: status,
                secondary: firstNonEmpty(row.statusDescription) || undefined,
                tone: 'status'
            };
        case 'Bio':
            return {
                typeKey,
                primary: firstNonEmpty(row.bio),
                tone: 'bio'
            };
        case 'Avatar':
            // The row's owner id is a raw `usr_…` identifier; only the avatar
            // name is meaningful on a card.
            return {
                typeKey,
                primary: firstNonEmpty(row.avatarName),
                tone: 'avatar'
            };
        default:
            return {
                typeKey,
                primary: location || status,
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

/** Parses the read model's `created_at`, which may use a space separator. */
export function parseFeedTimestamp(createdAt: string | null | undefined) {
    const trimmed = createdAt?.trim();
    if (!trimmed) {
        return null;
    }
    const parsed = new Date(
        trimmed.includes('T') ? trimmed : trimmed.replace(' ', 'T')
    );
    return Number.isNaN(parsed.getTime()) ? null : parsed;
}

export type MobileFeedTimeUnit = 'now' | 'minute' | 'hour' | 'clock';

/**
 * Relative time for a card. Returns a unit plus a count so the caller can
 * translate it; only the clock form ("03-14 12:30") is preformatted.
 */
export function mobileFeedTimeParts(
    createdAt: string | null | undefined,
    now: number = Date.now()
): { unit: MobileFeedTimeUnit; count?: number; clock?: string } | null {
    const parsed = parseFeedTimestamp(createdAt);
    if (!parsed) {
        return null;
    }
    const delta = now - parsed.getTime();
    if (delta < MINUTE) {
        return { unit: 'now' };
    }
    if (delta < HOUR) {
        return { unit: 'minute', count: Math.floor(delta / MINUTE) };
    }
    if (delta < DAY) {
        return { unit: 'hour', count: Math.floor(delta / HOUR) };
    }
    return {
        unit: 'clock',
        clock: `${pad(parsed.getMonth() + 1)}-${pad(parsed.getDate())} ${pad(parsed.getHours())}:${pad(parsed.getMinutes())}`
    };
}
