<script lang="ts">
  import { postMenu } from '$lib/state/postMenu.svelte';
  import PopoverMenu from './PopoverMenu.svelte';
  import ShareItems from './ShareItems.svelte';

  const open = $derived(postMenu.current);
  const anchor = $derived(open && { getBoundingClientRect: () => new DOMRect(open.x, open.y, 0, 0) });
</script>

<PopoverMenu {anchor} class="menu-md w-max" onclose={() => postMenu.close()}>
  {#if open}<ShareItems post={open.post} />{/if}
</PopoverMenu>
