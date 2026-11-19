<script module lang="ts">
  type Remembered = { ids: number[]; fetchChips: string[] };

  const rankings = new Map<string, Remembered>();
</script>

<script lang="ts">
  import { page } from '$app/state';
  import { api } from '$lib/api';
  import AlgorithmControls from '$lib/components/AlgorithmControls.svelte';
  import RankedView, { BATCH } from '$lib/components/RankedView.svelte';
  import { flushImpressions } from '$lib/impressions';
  import { flushSignals } from '$lib/signals';
  import { drawerLayout } from '$lib/screen';
  import { Loader } from '$lib/loader.svelte';
  import { onRerun } from '$lib/state/rerun.svelte';
  import { splitQuery, urlQuery } from '$lib/state/search.svelte';
  import { settings } from '$lib/state/settings.svelte';
  import { toast } from '$lib/state/toast.svelte';
  import { untrack } from 'svelte';

  const query = $derived(urlQuery(page.url));
  const key = $derived(`${query}\n${settings.hideLiked}\n${settings.forYouDiscover}`);
  const ranking = new Loader<number[]>();

  async function rank(): Promise<Remembered> {
    const chips = splitQuery(query);
    await flushImpressions();
    await flushSignals();
    const ranked = await api.forYou(chips, settings.hideLiked, settings.forYouDiscover);
    return { ids: ranked.slice(0, BATCH).map((entry) => entry.id), fetchChips: chips };
  }

  function load(rankingKey: string, fresh: boolean) {
    return ranking.run(async () => {
      const remembered = (!fresh && rankings.get(rankingKey)) || (await rank());
      rankings.set(rankingKey, remembered);
      return remembered.ids;
    });
  }

  $effect(() => {
    const rankingKey = key;
    untrack(() => load(rankingKey, false));
  });

  async function more(shown: number[]) {
    const remembered = rankings.get(key);
    if (!remembered) return [];
    await flushImpressions();
    await flushSignals();
    const found = await api.forYouMore(remembered.fetchChips, settings.hideLiked, settings.forYouDiscover, shown);
    if (found.broadened) {
      if (remembered.fetchChips.join(' ') === query) toast.show('Showing broader results…');
      remembered.fetchChips = found.broadened;
    }
    return found.ranked.slice(0, BATCH).map((entry) => entry.id);
  }

  onRerun(async () => {
    window.scrollTo({ top: 0, behavior: 'instant' });
    await load(key, true);
  });
</script>

{#if !drawerLayout.current}
  <div class="mb-2 flex gap-2"><AlgorithmControls page="forYou" /></div>
{/if}

<RankedView
  loader={ranking}
  keep={key}
  {more}
  loadingText="Finding recommendations…"
  emptyMessage="No recommendations match those tags."
  endMessage="That's all your top recommendations for now. Press Search for a fresh ranking."
/>
