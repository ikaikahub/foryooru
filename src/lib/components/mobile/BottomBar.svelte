<script lang="ts">
  import { appPath } from '$lib/navigation';
  import { rerunIfCurrent, tabHref, TABS } from '$lib/tabs';
  import TabIcon from '../sidebar/TabIcon.svelte';
</script>

<nav class="dock dock-sm z-(--z-tab-bar) bg-base-200" aria-label="Pages">
  {#each TABS as tab (tab.path)}
    {@const active = appPath() === tab.path}
    {@const href = tabHref(tab)}
    <a
      {href}
      draggable="false"
      aria-label={tab.label}
      aria-current={active ? 'page' : undefined}
      onclick={(e) => rerunIfCurrent(e, tab, href)}
      class={active ? 'text-primary' : 'opacity-60'}
    >
      <TabIcon {tab} class="size-5" />
      <span class="dock-label">{tab.label}</span>
    </a>
  {/each}
</nav>
