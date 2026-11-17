<script lang="ts">
  import { library } from '$lib/state/library.svelte';
  import { staged } from '$lib/state/search.svelte';
  import SavedSearches from './SavedSearches.svelte';

  const saved = $derived(library.savedQueries.has(staged.query));
</script>

<button
  type="button"
  class={['btn btn-square btn-sm', saved && 'btn-primary']}
  title={saved ? 'Saved: press to forget this search' : 'Save this search'}
  disabled={!staged.query}
  onclick={() => library.toggleSavedSearch(staged.query)}>🔖</button
>
<SavedSearches />
