import type { TagType } from '$lib/api';

const TEXT: Record<TagType, string> = {
    general: '',
    artist: 'text-tag-artist',
    character: 'text-tag-character',
    copyright: 'text-tag-copyright',
    metadata: 'text-tag-metadata',
    deprecated: 'text-tag-deprecated line-through',
};

const BORDER: Record<TagType, string> = {
    general: 'border-base-300',
    artist: 'border-tag-artist',
    character: 'border-tag-character',
    copyright: 'border-tag-copyright',
    metadata: 'border-tag-metadata',
    deprecated: 'border-tag-deprecated',
};

export const tagText = (type: TagType | null) => (type ? TEXT[type] : '');
export const tagBorder = (type: TagType | null) => (type ? BORDER[type] : 'border-base-300');
