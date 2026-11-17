import type { HiddenTag } from '$lib/api';

export function formatBytes(bytes: number) {
    const units = ['B', 'KB', 'MB', 'GB'];
    let unit = 0;
    while (bytes >= 1024 && unit < units.length - 1) {
        bytes /= 1024;
        unit++;
    }
    return `${bytes.toFixed(unit ? 1 : 0)} ${units[unit]}`;
}

export function hiddenNote(hidden: HiddenTag[]) {
    if (!hidden.length) return null;
    const total = hidden.reduce((sum, tag) => sum + tag.count, 0);
    const names = hidden.slice(0, 3).map((tag) => tag.name).join(', ');
    const more = hidden.length > 3 ? ` and ${hidden.length - 3} more` : '';
    return `Hidden: ${total} with blacklisted tags (${names}${more}).`;
}
