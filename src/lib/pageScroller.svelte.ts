import type { Attachment } from 'svelte/attachments';

let scroller = $state<HTMLElement | null>(null);

export const pageScroller: Attachment<HTMLElement> = (element) => {
    scroller = element;
    return () => {
        if (scroller === element) scroller = null;
    };
};

export function scrollPage(to: 'top' | 'bottom') {
    const target = scroller ?? document.documentElement;
    target.scrollTo({ top: to === 'top' ? 0 : target.scrollHeight, behavior: 'instant' });
}
