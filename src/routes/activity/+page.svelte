<script lang="ts">
  import EmptyState from '$lib/components/EmptyState.svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, imageUrl, type ActivityEntry, type ActivityFilter, type ActivityPage } from '$lib/api';
  import AlgorithmInfo from '$lib/components/AlgorithmInfo.svelte';
  import ErrorNote from '$lib/components/ErrorNote.svelte';
  import ExpandableText from '$lib/components/ExpandableText.svelte';
  import LoadingBar from '$lib/components/LoadingBar.svelte';
  import Pager from '$lib/components/Pager.svelte';
  import ScrollTable from '$lib/components/ScrollTable.svelte';
  import SelectMenu from '$lib/components/SelectMenu.svelte';
  import { Loader } from '$lib/loader.svelte';
  import { hrefWith, pageNumber } from '$lib/navigation';
  import { viewer } from '$lib/state/viewer.svelte';

  const FILTERS: { value: ActivityFilter; label: string }[] = [
    { value: 'all', label: 'Everything' },
    { value: 'views', label: 'Views' },
    { value: 'likes', label: 'Likes' },
    { value: 'dislikes', label: 'Dislikes' },
    { value: 'queries', label: 'API queries' },
  ];

  const LABELS: Record<ActivityEntry['kind'], string> = {
    view: '👁️ View',
    like: '❤️ Like',
    unlike: '❤️ Unlike',
    dislike: '💔 Dislike',
    undislike: '💔 Undislike',
    query: '🌐 Query',
  };

  const filter = $derived.by(() => {
    const requested = page.url.searchParams.get('type');
    return FILTERS.find((option) => option.value === requested)?.value ?? 'all';
  });
  const current = $derived(pageNumber(page.url));
  const activity = new Loader<ActivityPage>();
  let infoOpen = $state(false);

  const posts = $derived(activity.data?.entries.flatMap((entry) => (entry.kind === 'query' ? [] : [entry.post])) ?? []);

  $effect(() => {
    const [type, pageIndex] = [filter, current - 1];
    activity.run(() => api.activity(type, pageIndex));
  });

  const formatTime = (at: number) => new Date(at).toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'medium' });
  const seconds = (ms: number) => `${(ms / 1000).toFixed(1)}s`;
</script>

<div class="page-fill">
  <div class="mb-3 flex shrink-0 flex-wrap items-center gap-3">
    <h1 class="text-lg font-semibold">📜 Activity</h1>
    <SelectMenu
      value={filter}
      options={FILTERS}
      buttonClass="btn-sm w-44"
      title="Show"
      onchange={(type) => goto(hrefWith(page.url, { type, page: 1 }), { replaceState: true, noScroll: true })}
    />
    <button class="btn btn-square btn-sm" title="What affects recommendations" onclick={() => (infoOpen = true)}>ℹ️</button>
    {#if activity.data}<span class="text-sm opacity-60">{activity.data.total.toLocaleString()} entries</span>{/if}
  </div>

  {#if activity.loading}<LoadingBar text="Loading…" />{/if}
  {#if activity.error}<ErrorNote error={activity.error} centered={!activity.data} />{/if}
  {#if activity.data}
    {#if activity.data.entries.length}
      <ScrollTable>
        <thead>
          <tr><th>Time</th><th>Event</th><th>Post</th><th>Details</th></tr>
        </thead>
        <tbody>
          {#each activity.data.entries as entry (entry.id)}
            <tr>
              <td class="whitespace-nowrap opacity-70">{formatTime(entry.at)}</td>
              <td class="whitespace-nowrap">{LABELS[entry.kind]}</td>
              <td>
                {#if entry.kind !== 'query'}
                  {@const post = entry.post}
                  <button class="block cursor-pointer" title="Open #{post}" onclick={() => viewer.open(post, posts)}>
                    <img class="size-10 rounded object-cover" src={imageUrl('thumbnail', post)} alt="#{post}" loading="lazy" />
                  </button>
                {/if}
              </td>
              <td class="w-full min-w-72 max-w-0 text-sm">
                {#if entry.kind === 'view'}
                  <span class="capitalize">{entry.source}</span>
                  {#if entry.dwellMs !== null}· {seconds(entry.dwellMs)} on screen{/if}
                  {#if entry.quickSkip}<span class="badge badge-sm badge-warning ml-1">quick skip</span>{/if}
                  {#if entry.longLook}<span class="badge badge-sm badge-success ml-1">long look</span>{/if}
                {:else if entry.kind === 'query'}
                  <div class="flex items-start gap-2">
                    <div class="min-w-0 flex-1"><ExpandableText class="font-mono" text={entry.query || '(everything)'} /></div>
                    <span class="shrink-0 whitespace-nowrap">
                      <span class="opacity-70">{entry.endpoint}{entry.page !== null ? ` · page ${entry.page + 1}` : ''} · {entry.ms} ms ·</span>
                      {#if entry.error !== null}
                        <span class="text-error">{entry.error}</span>
                      {:else}
                        <span class={entry.status === 200 ? 'text-success' : 'text-error'}>{entry.status}</span>
                      {/if}
                    </span>
                  </div>
                {:else}
                  <span class="opacity-70">#{entry.post}</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </ScrollTable>
    {:else}
      <EmptyState message="Nothing recorded yet." />
    {/if}
    <Pager {current} pages={activity.data.pages} href={(number) => hrefWith(page.url, { page: number })} />
  {/if}
</div>

<AlgorithmInfo bind:open={infoOpen} />
