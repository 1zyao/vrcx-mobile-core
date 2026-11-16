import { useCallback, useEffect, useMemo, useState } from 'react';
import { Loader2, RefreshCw } from 'lucide-react';

import { cn } from '@/lib/utils';
import type { FeedRowOutput } from '@/platform/tauri/bindings';
import feedRepository from '@/repositories/feedRepository';
import { useRuntimeStore } from '@/state/runtimeStore';

import {
    describeMobileFeedRow,
    formatMobileFeedTime,
    type MobileFeedTone
} from './mobileFeedPresentation';

const FEED_PAGE_SIZE = 60;

const toneChipClass: Record<MobileFeedTone, string> = {
    online: 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400',
    offline: 'bg-slate-500/15 text-slate-600 dark:text-slate-400',
    move: 'bg-sky-500/15 text-sky-600 dark:text-sky-400',
    status: 'bg-amber-500/15 text-amber-600 dark:text-amber-400',
    bio: 'bg-violet-500/15 text-violet-600 dark:text-violet-400',
    avatar: 'bg-fuchsia-500/15 text-fuchsia-600 dark:text-fuchsia-400',
    unknown: 'bg-muted text-muted-foreground'
};

function feedRowKey(row: FeedRowOutput, index: number) {
    if (row.rowId != null) {
        return `r${row.rowId}`;
    }
    return `${row.type ?? 'x'}:${row.userId ?? 'x'}:${row.created_at ?? index}`;
}

function FeedAvatar({ row }: { row: FeedRowOutput }) {
    const url =
        row.currentAvatarThumbnailImageUrl?.trim() ||
        row.currentAvatarImageUrl?.trim() ||
        '';
    const initial = (row.displayName?.trim() || '?').slice(0, 1).toUpperCase();

    if (!url) {
        return (
            <div className="bg-muted text-muted-foreground flex size-10 shrink-0 items-center justify-center rounded-full text-sm font-medium">
                {initial}
            </div>
        );
    }

    return (
        <img
            src={url}
            alt=""
            loading="lazy"
            className="bg-muted size-10 shrink-0 rounded-full object-cover"
        />
    );
}

function FeedCard({ row }: { row: FeedRowOutput }) {
    const presentation = useMemo(() => describeMobileFeedRow(row), [row]);
    const time = formatMobileFeedTime(row.created_at);

    return (
        <article className="border-border/60 bg-card flex gap-3 border-b px-4 py-3 last:border-b-0">
            <FeedAvatar row={row} />
            <div className="min-w-0 flex-1">
                <div className="flex items-baseline gap-2">
                    <span className="truncate text-sm font-medium">
                        {row.displayName?.trim() || '未知用户'}
                    </span>
                    <span className="text-muted-foreground ml-auto shrink-0 text-xs">
                        {time}
                    </span>
                </div>
                <div className="mt-1 flex items-center gap-2">
                    <span
                        className={cn(
                            'shrink-0 rounded px-1.5 py-0.5 text-[11px] leading-4 font-medium',
                            toneChipClass[presentation.tone]
                        )}
                    >
                        {presentation.label}
                    </span>
                    <span className="truncate text-sm">
                        {presentation.primary}
                    </span>
                </div>
                {presentation.secondary ? (
                    <p className="text-muted-foreground mt-0.5 line-clamp-2 text-xs">
                        {presentation.secondary}
                    </p>
                ) : null}
            </div>
        </article>
    );
}

export function MobileFeedPage() {
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

    return (
        <div className="flex h-full min-h-0 flex-col">
            <div className="border-border/60 flex items-center justify-between border-b px-4 py-2">
                <span className="text-muted-foreground text-xs">
                    {loading ? '加载中…' : `${rows.length} 条动态`}
                </span>
                <button
                    type="button"
                    onClick={() => void load()}
                    disabled={loading}
                    className="text-muted-foreground hover:text-foreground active:text-foreground flex items-center gap-1 text-xs disabled:opacity-50"
                >
                    <RefreshCw
                        className={cn('size-3.5', loading && 'animate-spin')}
                    />
                    刷新
                </button>
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
                {loading && rows.length === 0 ? (
                    <div className="flex h-full items-center justify-center">
                        <Loader2 className="text-muted-foreground size-5 animate-spin" />
                    </div>
                ) : error ? (
                    <p className="text-muted-foreground p-6 text-center text-sm">
                        读取动态失败：{error}
                    </p>
                ) : rows.length === 0 ? (
                    <p className="text-muted-foreground p-6 text-center text-sm">
                        还没有动态
                    </p>
                ) : (
                    rows.map((row, index) => (
                        <FeedCard key={feedRowKey(row, index)} row={row} />
                    ))
                )}
            </div>
        </div>
    );
}
