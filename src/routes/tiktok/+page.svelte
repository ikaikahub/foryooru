<script lang="ts">
  import { page } from '$app/state';
  import Feed from '$lib/components/feed/Feed.svelte';
  import { splitQuery, urlQuery } from '$lib/state/search.svelte';

  const query = $derived(urlQuery(page.url));
  const initialPost = $derived(Number(page.url.searchParams.get('post')) || null);
</script>

{#key query}
  <Feed chips={splitQuery(query)} {initialPost} />
{/key}
