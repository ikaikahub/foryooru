export const NOTE = 'px-3 py-2.5 text-sm';

export const percent = (share: number | null) => (share === null ? '–' : `${Math.round(share * 100)}%`);
