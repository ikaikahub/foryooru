const SHOW_MS = 3000;

class Toast {
    message = $state<string | null>(null);
    #timer: ReturnType<typeof setTimeout> | undefined;

    show(message: string, duration = SHOW_MS) {
        this.message = message;
        clearTimeout(this.#timer);
        this.#timer = setTimeout(() => (this.message = null), duration);
    }
}

export const toast = new Toast();
