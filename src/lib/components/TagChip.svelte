<script lang="ts">
  import type { TagType } from '$lib/api';
  import { tagBorder } from '$lib/tags';
  import type { Snippet } from 'svelte';
  import TagLabel from './TagLabel.svelte';

  let {
    tag,
    type = null,
    title,
    onclick,
    children,
    blacklisted = false,
  }: {
    tag: string;
    type?: TagType | null;
    title?: string;
    onclick?: (e: MouseEvent) => void;
    children?: Snippet;
    blacklisted?: boolean;
  } = $props();
</script>

<span
  class={[
    'tag-chip inline-flex max-w-full items-center rounded-full border bg-base-100 text-sm',
    tag.startsWith('-') ? 'border-error' : tagBorder(type),
    blacklisted && 'opacity-60',
  ]}
>
  <button type="button" class="min-w-0 cursor-pointer py-0.5 pr-1 pl-2 text-left" {title} {onclick}>
    <TagLabel {tag} {type} {blacklisted} />
  </button>
  {@render children?.()}
</span>
