<script lang="ts">
  import { appPath } from '$lib/navigation';
  import { rerunIfCurrent, TABS, tabHref } from '$lib/tabs';
  import TabIcon from './TabIcon.svelte';
</script>

<nav class="flex flex-col items-center gap-1" aria-label="Pages">
  {#each TABS as tab (tab.path)}
    {@const active = appPath() === tab.path}
    {@const href = tabHref(tab)}
    <a
      {href}
      draggable="false"
      title={tab.title()}
      aria-label={tab.label}
      aria-current={active ? 'page' : undefined}
      onclick={(e) => rerunIfCurrent(e, tab, href)}
      class={[
        'btn btn-square btn-ghost relative text-lg',
        active
          ? 'bg-primary/25 hover:bg-primary/35 before:absolute before:inset-y-2 before:-left-1.5 before:w-1 before:rounded-full before:bg-primary'
          : 'opacity-60 hover:opacity-100',
      ]}
    >
      <TabIcon {tab} class="size-5" />
    </a>
  {/each}
</nav>
