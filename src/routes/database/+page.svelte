<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, type TablePage, type TableSize } from '$lib/api';
  import ErrorNote from '$lib/components/ErrorNote.svelte';
  import LoadingBar from '$lib/components/LoadingBar.svelte';
  import Pager from '$lib/components/Pager.svelte';
  import ScrollTable from '$lib/components/ScrollTable.svelte';
  import SelectMenu from '$lib/components/SelectMenu.svelte';
  import { Loader } from '$lib/loader.svelte';
  import { hrefWith, pageNumber } from '$lib/navigation';
  import { formatBytes } from '$lib/format';
  import { onMount } from 'svelte';

  const sizes = new Loader<TableSize[]>();
  const rows = new Loader<TablePage>();

  onMount(() => sizes.run(api.tableSizes));

  const options = $derived(sizes.data?.map((size) => ({ value: size.name, label: `${size.name} (${size.rows.toLocaleString()} · ${formatBytes(size.bytes)})` })) ?? []);
  const table = $derived.by(() => {
    const requested = page.url.searchParams.get('table');
    return options.find((option) => option.value === requested)?.value ?? options[0]?.value;
  });
  const current = $derived(pageNumber(page.url));

  $effect(() => {
    const [name, pageIndex] = [table, current - 1];
    if (name) rows.run(() => api.tablePage(name, pageIndex));
  });
</script>

<div class="page-fill">
  <div class="mb-3 flex shrink-0 flex-wrap items-center gap-3">
    <h1 class="text-lg font-semibold">🗃️ Database info</h1>
    {#if table}
      <SelectMenu
        value={table}
        {options}
        buttonClass="btn-sm w-64"
        menuClass="w-64"
        title="Table"
        onchange={(name) => goto(hrefWith(page.url, { table: name, page: 1 }), { replaceState: true, noScroll: true })}
      />
    {/if}
    <!-- {#if rows.data}<span class="text-sm opacity-60">{rows.data.total.toLocaleString()} rows</span>{/if} -->
  </div>

  {#if sizes.error}<ErrorNote error={sizes.error} centered={!sizes.data} />{/if}
  {#if rows.loading}<LoadingBar text="Loading…" />{/if}
  {#if rows.error}<ErrorNote error={rows.error} />{/if}
  {#if rows.data}
    {#if rows.data.rows.length}
      <ScrollTable class="font-mono text-xs">
          <thead>
            <tr>
              {#each rows.data.columns as column (column)}<th>{column}</th>{/each}
            </tr>
          </thead>
          <tbody>
            {#each rows.data.rows as row, index (index)}
              <tr>
                {#each row as value, column (column)}
                  <td title={value === null ? undefined : String(value)}>
                    {#if value === null}<span class="opacity-40">NULL</span>{:else}<div class="max-w-xs truncate">{value}</div>{/if}
                  </td>
                {/each}
              </tr>
            {/each}
          </tbody>
      </ScrollTable>
    {:else}
      <p class="opacity-70">This table is empty.</p>
    {/if}
    <Pager {current} pages={rows.data.pages} href={(number) => hrefWith(page.url, { page: number })} />
  {/if}
</div>
