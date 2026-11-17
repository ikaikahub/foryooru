<script lang="ts">
  import { DEFAULT_SIDEBAR_WIDTH, settings } from '$lib/state/settings.svelte';
  import type { Snippet } from 'svelte';
  import CollapseTab from '../CollapseTab.svelte';
  import ModeTabs from './ModeTabs.svelte';

  let { children }: { children: Snippet } = $props();

  const MIN_WIDTH = 300;
  const MAX_WIDTH = 600;

  let resizing = $state(false);
  let railWidth = $state(0);

  const clamp = (width: number) => Math.round(Math.max(MIN_WIDTH, Math.min(width, MAX_WIDTH, window.innerWidth - 60)));

  function start(e: PointerEvent & { currentTarget: HTMLElement }) {
    if (settings.sidebarCollapsed) return;
    e.preventDefault();
    resizing = true;
    e.currentTarget.setPointerCapture(e.pointerId);
  }

  function move(e: PointerEvent) {
    if (resizing) settings.sidebarWidth = clamp(e.clientX - railWidth);
  }
</script>

<div class="sticky top-0 z-(--z-sidebar) flex h-dvh w-fit shrink-0">
  <div
    class="flex h-full w-(--rail-width) shrink-0 flex-col items-center border-r border-base-300 bg-base-200 py-2"
    bind:clientWidth={railWidth}
  >
    <ModeTabs />
  </div>
  {#if !settings.sidebarCollapsed}
    <aside class="flex h-full shrink-0 flex-col bg-base-200 px-3" style:width="{settings.sidebarWidth}px">
      <div class="min-h-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain py-3 no-scrollbar">
        {@render children()}
      </div>
    </aside>
  {/if}
</div>
<div class="fixed inset-y-0 z-(--z-sidebar)" style:left="calc(var(--rail-width) + {settings.sidebarCollapsed ? 0 : settings.sidebarWidth}px)">
  <div
    class={['h-full w-1 bg-base-300', !settings.sidebarCollapsed && 'cursor-col-resize touch-none hover:bg-primary', resizing && 'bg-primary']}
    role="separator"
    aria-orientation="vertical"
    title={settings.sidebarCollapsed ? undefined : 'Drag to resize · double-click to reset'}
    onpointerdown={start}
    onpointermove={move}
    onpointerup={() => (resizing = false)}
    ondblclick={() => (settings.sidebarWidth = DEFAULT_SIDEBAR_WIDTH)}
  ></div>
  <CollapseTab
    title={settings.sidebarCollapsed ? 'Show sidebar' : 'Hide sidebar'}
    onclick={() => (settings.sidebarCollapsed = !settings.sidebarCollapsed)}
  >
    {settings.sidebarCollapsed ? '▶' : '◀'}
  </CollapseTab>
</div>
