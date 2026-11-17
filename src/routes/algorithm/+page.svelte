<script lang="ts">
  import { api, type AlgorithmOverview } from '$lib/api';
  import FetchAccuracy from '$lib/components/algorithm/FetchAccuracy.svelte';
  import LearningModel from '$lib/components/algorithm/LearningModel.svelte';
  import ErrorNote from '$lib/components/ErrorNote.svelte';
  import GroupColumns from '$lib/components/GroupColumns.svelte';
  import LoadingBar from '$lib/components/LoadingBar.svelte';
  import AlgorithmGroup from '$lib/components/settings/AlgorithmGroup.svelte';
  import { Loader } from '$lib/loader.svelte';
  import { rememberedSearch, splitQuery } from '$lib/state/search.svelte';
  import { settings } from '$lib/state/settings.svelte';
  import { onMount } from 'svelte';

  const overview = new Loader<AlgorithmOverview>();

  onMount(() =>
    overview.run(() => api.algorithmOverview(splitQuery(rememberedSearch('/for-you')), settings.hideLiked)),
  );
</script>

<div class="mb-3 flex flex-wrap items-center gap-3">
  <h1 class="text-lg font-semibold">📊 Algorithm</h1>
  {#if overview.data}
    {@const { posts, candidates, unseen } = overview.data}
    <span class="text-sm opacity-70">
      {posts.toLocaleString()} posts stored · For you has {unseen.toLocaleString()} unseen of {candidates.toLocaleString()} matching your search
    </span>
  {/if}
</div>

{#if overview.loading}<LoadingBar text="Loading…" />{/if}
{#if overview.error}<ErrorNote error={overview.error} centered={!overview.data} />{/if}

<GroupColumns>
  {#if overview.data}
    <LearningModel status={overview.data.model} />
    <FetchAccuracy stats={overview.data.fetching} />
  {/if}
  <AlgorithmGroup />
</GroupColumns>
