import { afterNavigate, beforeNavigate } from '$app/navigation';

const RESTORE_TIMEOUT_MS = 3000;

const saved = new Map<string, { href: string; top: number }>();
let stopRestoring: (() => void) | null = null;

function restore(top: number) {
    stopRestoring?.();
    const root = document.documentElement;
    const observer = new ResizeObserver(attempt);
    const timer = setTimeout(stop, RESTORE_TIMEOUT_MS);

    function attempt() {
        if (root.scrollHeight - innerHeight < top) return;
        scrollTo({ top, behavior: 'instant' });
        stop();
    }

    function stop() {
        observer.disconnect();
        clearTimeout(timer);
        removeEventListener('wheel', stop);
        removeEventListener('touchstart', stop);
        stopRestoring = null;
    }

    observer.observe(document.body);
    addEventListener('wheel', stop, { passive: true });
    addEventListener('touchstart', stop, { passive: true });
    stopRestoring = stop;
    attempt();
}

export function rememberScroll() {
    beforeNavigate(({ from }) => {
        stopRestoring?.();
        if (from) saved.set(from.url.pathname, { href: from.url.href, top: scrollY });
    });

    afterNavigate(({ to }) => {
        const entry = to && saved.get(to.url.pathname);
        if (entry && entry.href === to.url.href && entry.top > 0) restore(entry.top);
    });
}
