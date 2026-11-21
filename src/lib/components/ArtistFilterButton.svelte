<script lang="ts">
  import { page } from '$app/state';
  import { FILTER_PARAM, splitQuery } from '$lib/state/search.svelte';

  // the menu this opens lives on the Artists page itself, so the button can sit in the top bar on mobile
  const chips = $derived(splitQuery(page.url.searchParams.get(FILTER_PARAM) ?? ''));
</script>

<button
  class={['btn btn-square btn-sm', chips.length && 'btn-primary']}
  popovertarget="artist-filter"
  style:anchor-name="--artist-filter"
  aria-label="Filter and sort"
  title={chips.length ? `Filtered: ${chips.join(' ')}` : 'Filter and sort'}
>
  <!-- a funnel -->
  <svg class="size-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d="M3 5h18l-7 8v6l-4 2v-8z" />
  </svg>
</button>
