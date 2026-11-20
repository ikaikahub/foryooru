<script lang="ts">
  import { appPath } from '$lib/navigation';
  import { rerunIfCurrent, tabHref, TABS } from '$lib/tabs';
  import BottomSheet from '../BottomSheet.svelte';
  import TabIcon from '../sidebar/TabIcon.svelte';

  // the first four tabs fill the mobile bar, the remaining pages live under More
  const barTabs = TABS.slice(0, 4);
  const moreTabs = TABS.slice(4);

  let moreOpen = $state(false);
  const moreActive = $derived(moreTabs.some((tab) => tab.path === appPath()));
</script>

<nav class="dock dock-sm z-(--z-tab-bar) bg-base-200" aria-label="Pages">
  {#each barTabs as tab (tab.path)}
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
  <button
    type="button"
    class={[moreActive ? 'dock-active text-primary' : 'opacity-60']}
    aria-label="More pages"
    onclick={() => (moreOpen = true)}
  >
    <span class="text-lg leading-none">⋯</span>
    <span class="dock-label">More</span>
  </button>
</nav>

<BottomSheet bind:open={moreOpen} title="More">
  <ul class="menu w-full gap-1">
    {#each moreTabs as tab (tab.path)}
      {@const href = tabHref(tab)}
      <li>
        <a
          {href}
          onclick={(e) => {
            moreOpen = false;
            rerunIfCurrent(e, tab, href);
          }}><TabIcon {tab} class="size-5" />{tab.label}</a
        >
      </li>
    {/each}
  </ul>
</BottomSheet>
