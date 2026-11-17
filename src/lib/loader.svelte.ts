export class Loader<T> {
    data = $state<T | null>(null);
    error = $state<unknown>(null);
    loading = $state(false);
    private latest = 0;

    async run(load: () => Promise<T>) {
        const request = ++this.latest;
        this.loading = true;
        this.error = null;
        try {
            const data = await load();
            if (request === this.latest) this.data = data;
        } catch (error) {
            if (request === this.latest) this.error = error;
        } finally {
            if (request === this.latest) this.loading = false;
        }
    }
}
