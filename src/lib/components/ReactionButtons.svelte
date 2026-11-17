<script lang="ts">
  import { library } from '$lib/state/library.svelte';
  import type { ClassValue } from 'svelte/elements';

  let { id, class: className, bare = false }: { id: number; class?: ClassValue; bare?: boolean } = $props();

  const reaction = $derived(library.reaction(id));

  const MUTED = 'grayscale-[.45] hover:grayscale-0';
  const BARE_MUTED = `opacity-0 group-hover:opacity-100 focus-visible:opacity-100 [@media(hover:none)]:opacity-100 hover:scale-110 ${MUTED}`;

  const look = (active: boolean, highlight: string): ClassValue =>
    bare
      ? ['btn-ghost border-0 bg-transparent shadow-none drop-shadow-[0_1px_2px_rgb(0_0_0/0.85)] hover:bg-transparent active:bg-transparent', !active && BARE_MUTED]
      : active
        ? highlight
        : MUTED;
</script>

<button
  type="button"
  class={['btn', className, look(reaction.liked, 'border-liked bg-liked/30')]}
  aria-pressed={reaction.liked}
  title={reaction.liked ? 'Unlike (f)' : 'Like (f)'}
  onclick={() => library.toggleLike(id)}>❤️</button
>
<button
  type="button"
  class={['btn', className, look(reaction.disliked, 'border-disliked bg-disliked/35')]}
  aria-pressed={reaction.disliked}
  title={reaction.disliked ? 'Undo dislike (d)' : 'Dislike: show fewer like it (d)'}
  onclick={() => library.toggleDislike(id)}>💔</button
>
