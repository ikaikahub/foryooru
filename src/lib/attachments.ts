import { addImpression } from '$lib/impressions';
import type { Attachment } from 'svelte/attachments';

const RETRY_DELAYS = [1000, 2000, 4000, 8000, 15000, 30000];
const SEEN_AFTER_MS = 1000;
export const PRELOAD_AHEAD = 8;

const visibleCallbacks = new WeakMap<Element, () => void>();
const visibleWatcher = new IntersectionObserver((entries) => {
    for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        visibleWatcher.unobserve(entry.target);
        visibleCallbacks.get(entry.target)?.();
    }
});

export function onVisible(callback: () => void): Attachment {
    return (element) => {
        visibleCallbacks.set(element, callback);
        visibleWatcher.observe(element);
        return () => visibleWatcher.unobserve(element);
    };
}

const visibilityCallbacks = new WeakMap<Element, (visible: boolean) => void>();
const visibilityWatcher = new IntersectionObserver((entries) => {
    for (const entry of entries) visibilityCallbacks.get(entry.target)?.(entry.isIntersecting);
});

export function whileVisible(callback: (visible: boolean) => void): Attachment {
    return (element) => {
        visibilityCallbacks.set(element, callback);
        visibilityWatcher.observe(element);
        return () => {
            visibilityWatcher.unobserve(element);
            visibilityCallbacks.delete(element);
        };
    };
}

export function image(src: string, load = true): Attachment<HTMLImageElement> {
    return (img) => {
        let attempt = 0;
        let timer: ReturnType<typeof setTimeout> | undefined;
        const retry = () => {
            if (attempt >= RETRY_DELAYS.length) return;
            img.dataset.loading = '';
            const delay = RETRY_DELAYS[attempt] * (0.75 + Math.random() / 2);
            attempt += 1;
            timer = setTimeout(() => (img.src = `${src}?retry=${attempt}`), delay);
        };
        const loaded = () => delete img.dataset.loading;
        img.classList.add('fade-in');
        img.addEventListener('error', retry);
        img.addEventListener('load', loaded);
        if (load) img.src = src;
        if (!img.complete) img.dataset.loading = '';
        return () => {
            clearTimeout(timer);
            img.removeEventListener('error', retry);
            img.removeEventListener('load', loaded);
        };
    };
}

export const playWhile =
    (playing: boolean): Attachment<HTMLVideoElement> =>
    (video) => {
        if (playing) video.play().catch(() => {});
        else video.pause();
    };

const seenTimers = new WeakMap<Element, ReturnType<typeof setTimeout>>();
const seenPosts = new WeakMap<Element, number>();
const seenWatcher = new IntersectionObserver(
    (entries) => {
        for (const entry of entries) {
            const card = entry.target;
            clearTimeout(seenTimers.get(card));
            if (!entry.isIntersecting) continue;
            seenTimers.set(
                card,
                setTimeout(() => {
                    seenWatcher.unobserve(card);
                    const post = seenPosts.get(card);
                    if (post !== undefined) addImpression({ post, source: 'grid' });
                }, SEEN_AFTER_MS),
            );
        }
    },
    { threshold: 0.5 },
);

export function seen(post: number): Attachment {
    return (card) => {
        seenPosts.set(card, post);
        seenWatcher.observe(card);
        return () => {
            clearTimeout(seenTimers.get(card));
            seenWatcher.unobserve(card);
        };
    };
}

type TouchHandlers = { start: (e: TouchEvent) => void; move: (e: TouchEvent) => void; end: () => void };

export function touchDrag(handlers: TouchHandlers): Attachment<HTMLElement> {
    return (element) => {
        const end = () => handlers.end();
        element.addEventListener('touchstart', handlers.start, { passive: true });
        element.addEventListener('touchmove', handlers.move, { passive: false });
        element.addEventListener('touchend', end);
        element.addEventListener('touchcancel', end);
        return () => {
            element.removeEventListener('touchstart', handlers.start);
            element.removeEventListener('touchmove', handlers.move);
            element.removeEventListener('touchend', end);
            element.removeEventListener('touchcancel', end);
        };
    };
}

const DOUBLE_TAP_MS = 300;
const TAP_SLOP_PX = 20;
const NOT_TAPPABLE = 'button, a, input, video[controls]';

type TapHandlers = { double: () => void; single?: () => void };

export function taps(handlers: TapHandlers): Attachment<HTMLElement> {
    return (element) => {
        let down: { x: number; y: number } | null = null;
        let lastTap: { at: number; x: number; y: number } | null = null;
        let singleTimer: ReturnType<typeof setTimeout> | undefined;

        function onpointerdown(e: PointerEvent) {
            const tappable = e.isPrimary && e.button === 0 && !(e.target instanceof Element && e.target.closest(NOT_TAPPABLE));
            down = tappable ? { x: e.clientX, y: e.clientY } : null;
            if (!tappable) lastTap = null;
        }

        function onpointerup(e: PointerEvent) {
            const start = down;
            down = null;
            if (!start || Math.hypot(e.clientX - start.x, e.clientY - start.y) > TAP_SLOP_PX) {
                lastTap = null;
                return;
            }
            const close = lastTap && e.timeStamp - lastTap.at < DOUBLE_TAP_MS && Math.hypot(e.clientX - lastTap.x, e.clientY - lastTap.y) < TAP_SLOP_PX * 2;
            clearTimeout(singleTimer);
            if (close) {
                lastTap = null;
                handlers.double();
                return;
            }
            lastTap = { at: e.timeStamp, x: e.clientX, y: e.clientY };
            const single = handlers.single;
            if (single) singleTimer = setTimeout(single, DOUBLE_TAP_MS);
        }

        element.addEventListener('pointerdown', onpointerdown);
        element.addEventListener('pointerup', onpointerup);
        return () => {
            clearTimeout(singleTimer);
            element.removeEventListener('pointerdown', onpointerdown);
            element.removeEventListener('pointerup', onpointerup);
        };
    };
}
