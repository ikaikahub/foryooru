import type { Post } from '$lib/api';

type OpenMenu = { post: Post; x: number; y: number };

class PostMenu {
    current = $state<OpenMenu | null>(null);

    open(e: MouseEvent, post: Post) {
        e.preventDefault();
        const show = () => (this.current = { post, x: e.clientX, y: e.clientY });
        if (e.buttons === 0) show();
        else window.addEventListener('pointerup', () => setTimeout(show), { once: true });
    }

    close() {
        this.current = null;
    }
}

export const postMenu = new PostMenu();
