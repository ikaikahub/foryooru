<script lang="ts">
  import type { PostDetail, Site } from '$lib/api';
  import { searchSimilar } from '$lib/postActions';
  import { settings } from '$lib/state/settings.svelte';
  import PopoverMenu from './PopoverMenu.svelte';
  import ReactionButtons from './ReactionButtons.svelte';
  import ShareItems from './ShareItems.svelte';

  let { post, site = settings.site }: { post: PostDetail; site?: Site } = $props();

  const BUTTON = 'btn btn-sm';
  // likes, dislikes and similar all read or write this site's database, which has no row for the other site's posts
  const own = $derived(site === settings.site);

  let shareAnchor = $state<HTMLElement | null>(null);
</script>

{#if own}
  <ReactionButtons id={post.id} class="btn-sm" />
  <button class={BUTTON} title="Search for posts like this one" onclick={() => searchSimilar(post)}>🔍 Similar</button>
{/if}
<button class={BUTTON} title="Share or save" onclick={(e) => (shareAnchor = shareAnchor ? null : e.currentTarget)}>📤 Share</button>
<PopoverMenu anchor={shareAnchor} placement="above" class="menu-md w-max" onclose={() => (shareAnchor = null)}>
  <ShareItems {post} {site} />
</PopoverMenu>
