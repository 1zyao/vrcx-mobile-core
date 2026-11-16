import { Fragment, useCallback, useEffect, useMemo, useState } from 'react';
import { Loader2, RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { cn } from '@/lib/utils';
import type { FeedRowOutput } from '@/platform/tauri/bindings';
import feedRepository from '@/repositories/feedRepository';
import { useRuntimeStore } from '@/state/runtimeStore';

import {
    describeMobileFeedRow,
    mobileFeedTimeParts,
    parseFeedTimestamp,
    type MobileFeedTone
} from './mobileFeedPresentation';

const FEED_PAGE_SIZE = 60;

const toneChipClass: Record<MobileFeedTone, string> = {
    online: 'bg-emerald-500/12 text-emerald-700 dark:text-emerald-300',
    offline: 'bg-slate-500/12 text-slate-600 dark:text-slate-300',
    move: 'bg-sky-500/12 text-sky-700 dark:text-sky-300',
    status: 'bg-amber-500/15 text-amber-700 dark:text-amber-300',
    bio: 'bg-violet-500/12 text-violet-700 dark:text-violet-300',
    avatar: 'bg-fuchsia-500/12 text-fuchsia-700 dark:text-fuchsia-300',
    unknown: 'bg-muted text-muted-foreground'
};

function feedRowKey(row: FeedRowOutput, index: number) {
    if (row.rowId != null) {
        return `r${row.rowId}`;
    }
    return `${row.type ?? 'x'}:${row.userId ?? 'x'}:${row.created_at ?? index}`;
}

type DayGroup = {
    kind: 'today' | 'yesterday' | 'date';
    date: Date | null;
    rows: FeedRowOutput[];
};

function groupByDay(rows: FeedRowOutput[]): DayGroup[] {
    const startOfDay = (date: Date) =>
        new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
    const today = startOfDay(new Date());

    const groups: DayGroup[] = [];
    for (const row of rows) {
        const parsed = parseFeedTimestamp(row.created_at);
        const kind: DayGroup['kind'] = parsed
            ? Math.round((today - startOfDay(parsed)) / 86_400_000) === 0
                ? 'today'
                : Math.round((today - startOfDay(parsed)) / 86_400_000) === 1
                  ? 'yesterday'
                  : 'date'
            : 'date';
        const last = groups.at(-1);
        const sameGroup =
            last &&
            last.kind === kind &&
            (kind !== 'date' ||
                (last.date &&
                    parsed &&
                    startOfDay(last.date) === startOfDay(parsed)));
        if (sameGroup) {
            last.rows.push(row);
        } else {
            groups.push({ kind, date: parsed, rows: [row] });
        }
    }
    return groups;
}

/** Stable per-user hue so the same person keeps the same fallback colour. */
function avatarHue(seed: string) {
    let hash = 0;
    for (let index = 0; index < seed.length; index += 1) {
        hash = (hash * 31 + seed.charCodeAt(index)) | 0;
    }
    return Math.abs(hash) % 360;
}

function FeedAvatar({ row }: { row: FeedRowOutput }) {
    const url =
        row.currentAvatarThumbnailImageUrl?.trim() ||
        row.currentAvatarImageUrl?.trim() ||
        '';
    const [status, setStatus] = useState<'loading' | 'ok' | 'failed'>(
        url ? 'loading' : 'failed'
    );
    const hue = avatarHue(row.userId ?? row.displayName ?? '?');
    const initial = (row.displayName?.trim() || '?').slice(0, 1).toUpperCase();

    return (
        <div className="shrink-0">
            {url && status !== 'failed' ? (
                <img
                    src={url}
                    alt=""
                    loading="lazy"
                    onLoad={() => setStatus('ok')}
                    onError={() => setStatus('failed')}
                    className={cn(
                        'bg-muted size-11 rounded-full object-cover',
                        status === 'loading' && 'animate-pulse'
                    )}
                />
            ) : (
                <div
                    style={{
                        backgroundColor: `hsl(${hue} 55% 88%)`,
                        color: `hsl(${hue} 45% 30%)`
                    }}
                    className="flex size-11 items-center justify-center rounded-full text-[15px] font-semibold"
                >
                    {initial}
                </div>
            )}
        </div>
    );
}

function FeedRow({ row }: { row: FeedRowOutput }) {
    const { t, i18n } = useTranslation();
    const presentation = useMemo(() => describeMobileFeedRow(row), [row]);

    const relativeTime = useMemo(() => {
        const parts = mobileFeedTimeParts(row.created_at);
        if (!parts) {
            return '';
        }
        if (parts.unit === 'clock') {
            return parts.clock ?? '';
        }
        const formatter = new Intl.RelativeTimeFormat(i18n.language, {
            numeric: 'auto',
            style: 'short'
        });
        if (parts.unit === 'now') {
            return formatter.format(0, 'second');
        }
        return formatter.format(
            -(parts.count ?? 0),
            parts.unit === 'minute' ? 'minute' : 'hour'
        );
    }, [row.created_at, i18n.language]);

    const typeLabel = t(`view.feed.filters.${presentation.typeKey}`, {
        defaultValue: presentation.typeKey
    });
    const primary = presentation.primaryKey
        ? t(presentation.primaryKey)
        : presentation.primary;
    // Falls back to the instance access level when the world is not cached,
    // so a row never shows a raw id or "unknown world".
    const accessLabel = presentation.accessKey
        ? t(presentation.accessKey)
        : '';

    return (
        <div className="flex items-start gap-3 px-4 py-3">
            <FeedAvatar row={row} />
            <div className="min-w-0 flex-1">
                <div className="flex items-baseline gap-2">
                    <span className="truncate text-[15px] leading-5 font-semibold">
                        {row.displayName?.trim() || '—'}
                    </span>
                    <span className="text-muted-foreground ml-auto shrink-0 text-xs tabular-nums">
                        {relativeTime}
                    </span>
                </div>
                <div className="mt-1 flex items-center gap-1.5">
                    <span
                        className={cn(
                            'shrink-0 rounded-md px-1.5 py-0.5 text-[11px] leading-4 font-medium',
                            toneChipClass[presentation.tone]
                        )}
                    >
                        {typeLabel}
                    </span>
                    {primary ? (
                        <span className="truncate text-sm leading-5">
                            {primary}
                        </span>
                    ) : null}
                    {accessLabel ? (
                        <span className="text-muted-foreground shrink-0 text-xs">
                            {accessLabel}
                        </span>
                    ) : null}
                </div>
                {presentation.secondary ? (
                    <p className="text-muted-foreground mt-0.5 line-clamp-1 text-xs leading-4">
                        {presentation.secondary}
                    </p>
                ) : null}
            </div>
        </div>
    );
}

export function MobileFeedPage() {
    const { t, i18n } = useTranslation();
    const currentUserId = useRuntimeStore((state) => state.auth.currentUserId);
    const [rows, setRows] = useState<FeedRowOutput[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    const load = useCallback(async () => {
        if (!currentUserId) {
            setRows([]);
            setLoading(false);
            return;
        }
        setLoading(true);
        setError(null);
        try {
            const page = await feedRepository.queryFeedPage({
                userId: currentUserId,
                maxEntries: FEED_PAGE_SIZE
            });
            setRows(page);
        } catch (cause) {
            setError(cause instanceof Error ? cause.message : String(cause));
            setRows([]);
        } finally {
            setLoading(false);
        }
    }, [currentUserId]);

    useEffect(() => {
        void load();
    }, [load]);

    const groups = useMemo(() => groupByDay(rows), [rows]);

    const groupTitle = useCallback(
        (group: DayGroup) => {
            if (group.kind === 'today') {
                return t('browse_history.date.today');
            }
            if (group.kind === 'yesterday') {
                return t('browse_history.date.yesterday');
            }
            if (!group.date) {
                return '';
            }
            return new Intl.DateTimeFormat(i18n.language, {
                month: 'numeric',
                day: 'numeric'
            }).format(group.date);
        },
        [t, i18n.language]
    );

    return (
        <div className="bg-muted/40 flex h-full min-h-0 flex-col">
            <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 pt-2 pb-24">
                {loading && rows.length === 0 ? (
                    <div className="flex h-full items-center justify-center">
                        <Loader2 className="text-muted-foreground size-5 animate-spin" />
                    </div>
                ) : error ? (
                    <p className="text-muted-foreground p-6 text-center text-sm">
                        {error}
                    </p>
                ) : rows.length === 0 ? (
                    <p className="text-muted-foreground p-6 text-center text-sm">
                        {t('common.no_more')}
                    </p>
                ) : (
                    groups.map((group, groupIndex) => (
                        <section
                            key={`${group.kind}-${group.rows[0]?.rowId ?? group.rows[0]?.created_at ?? ''}`}
                            className="mb-4"
                        >
                            <div className="flex items-center justify-between px-1 pb-1.5">
                                <h2 className="text-muted-foreground text-[13px] font-medium">
                                    {groupTitle(group)}
                                </h2>
                                {groupIndex === 0 ? (
                                    <button
                                        type="button"
                                        aria-label={t('common.actions.refresh')}
                                        onClick={() => void load()}
                                        disabled={loading}
                                        className="text-muted-foreground active:text-foreground -mr-1 flex size-7 items-center justify-center rounded-full disabled:opacity-50"
                                    >
                                        <RefreshCw
                                            className={cn(
                                                'size-3.5',
                                                loading && 'animate-spin'
                                            )}
                                        />
                                    </button>
                                ) : null}
                            </div>
                            {/* Inset grouped card: one rounded surface per
                                section, hairline dividers aligned with the text
                                column instead of the card edge. */}
                            <div className="bg-card overflow-hidden rounded-xl shadow-sm">
                                {group.rows.map((row, index) => (
                                    <Fragment key={feedRowKey(row, index)}>
                                        {index > 0 ? (
                                            <div className="bg-border/50 ml-[4.5rem] h-px" />
                                        ) : null}
                                        <FeedRow row={row} />
                                    </Fragment>
                                ))}
                            </div>
                        </section>
                    ))
                )}
            </div>
        </div>
    );
}
