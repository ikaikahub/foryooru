export function isShortcut(e: KeyboardEvent) {
    if (e.ctrlKey || e.metaKey || e.altKey) return false;
    return !(e.target as HTMLElement).closest('input, textarea, select');
}
