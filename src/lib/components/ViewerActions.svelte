<script lang="ts">
  import type { PostDetail } from '$lib/api';
  import { searchSimilar } from '$lib/postActions';
  import PopoverMenu from './PopoverMenu.svelte';
  import ReactionButtons from './ReactionButtons.svelte';
  import ShareItems from './ShareItems.svelte';

  let { post }: { post: PostDetail } = $props();

  const BUTTON = 'btn btn-sm';

  let shareAnchor = $state<HTMLElement | null>(null);
</script>

<ReactionButtons id={post.id} class="btn-sm" />
<button class={BUTTON} title="Search for posts like this one" onclick={() => searchSimilar(post)}>🔍 Similar</button>
<button class={BUTTON} title="Share or save" onclick={(e) => (shareAnchor = shareAnchor ? null : e.currentTarget)}>📤 Share</button>
<PopoverMenu anchor={shareAnchor} placement="above" class="menu-md w-max" onclose={() => (shareAnchor = null)}>
  <ShareItems {post} />
</PopoverMenu>
