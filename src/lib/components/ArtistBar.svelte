<script lang="ts">
  import type { PostDetail } from '$lib/api';
  import { signal } from '$lib/signals';
  import { library } from '$lib/state/library.svelte';
  import type { TagTarget } from '$lib/state/search.svelte';
  import { tagMenu } from '$lib/state/tagMenu.svelte';
  import { tagText } from '$lib/tags';

  let { post, target, class: className = '' }: { post: PostDetail; target: TagTarget; class?: string } = $props();

  let expanded = $state(false);
  const artists = $derived(post.tagGroups.find((group) => group.label === 'Artist')?.tags ?? []);
  const shown = $derived(expanded ? artists : artists.slice(0, 1));

  $effect(() => {
    void post.id;
    expanded = false;
  });
</script>

{#snippet follow(name: string)}
  {@const following = library.followed.has(name)}
  <button
    class={['btn btn-xs', following ? 'btn-ghost border-white/40 text-white' : 'btn-primary']}
    aria-pressed={following}
    onclick={() => library.toggleTag('followedArtists', name)}
  >
    {following ? 'Following' : 'Follow'}
  </button>
{/snippet}

{#if artists.length}
  <div class={['flex flex-col gap-1 text-sm text-white', className]}>
    {#each shown as artist (artist.name)}
      <div class="flex min-w-0 items-center gap-2">
        <!-- opens the same menu as the artist in the tag list -->
        <button
          type="button"
          class={['cursor-pointer truncate font-semibold [text-shadow:0_1px_2px_rgb(0_0_0/0.8)] hover:underline', tagText(artist.type)]}
          onclick={(e) => {
            signal(post.id, 'tag', artist.name);
            tagMenu.show(e.currentTarget, artist.name, artist.type, target);
          }}
        >
          🎨 {artist.name.replaceAll('_', ' ')}
        </button>
        {@render follow(artist.name)}
      </div>
    {/each}
    {#if artists.length > 1}
      <button class="self-start text-xs opacity-80 hover:opacity-100" onclick={() => (expanded = !expanded)}>
        {expanded ? 'Show less' : `and ${artists.length - 1} more`}
      </button>
    {/if}
  </div>
{/if}
