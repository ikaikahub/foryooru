<script lang="ts">
  import { page } from '$app/state';
  import { FLICK_SPEED, SPRING, swipeAxis, swipeSpeed, trackMove, type Moves } from '$lib/motion';
  import { appPath, closeLayer, openLayer, openOrRerun } from '$lib/navigation';
  import { viewer } from '$lib/state/viewer.svelte';
  import { TABS, tabHref, type Tab } from '$lib/tabs';
  import { onMount, untrack } from 'svelte';
  import { Spring } from 'svelte/motion';
  import TabIcon from '../sidebar/TabIcon.svelte';

  const EDGE_PX = 32;

  const shown = new Spring(0, SPRING);
  let width = $state(0);
  let drag: { x0: number; y0: number; from: number; axis: 'x' | 'y' | null; moves: Moves } | null = null;

  const open = $derived(page.state.menuOpen === true);

  $effect(() => {
    const target = open ? 1 : 0;
    untrack(() => {
      if (!drag) shown.target = target;
    });
  });

  async function pick(e: MouseEvent, tab: Tab) {
    e.preventDefault();
    const href = tabHref(tab);
    await closeLayer('menuOpen');
    openOrRerun(href);
  }

  onMount(() => {
    function ontouchstart(e: TouchEvent) {
      const touch = e.touches[0];
      const allowed = e.touches.length === 1 && viewer.current === null && !page.state.searching && (open || touch.clientX <= EDGE_PX);
      drag = allowed ? { x0: touch.clientX, y0: touch.clientY, from: shown.current, axis: null, moves: [] } : null;
    }

    function ontouchmove(e: TouchEvent) {
      if (!drag || !width) return;
      const touch = e.touches[0];
      const [dx, dy] = [touch.clientX - drag.x0, touch.clientY - drag.y0];
      drag.axis ??= swipeAxis(dx, dy);
      if (drag.axis !== 'x') return;
      e.preventDefault();
      shown.set(Math.min(1, Math.max(0, drag.from + dx / width)), { instant: true });
      drag.moves = trackMove(drag.moves, e.timeStamp, touch.clientX);
    }

    function ontouchend() {
      const finished = drag;
      drag = null;
      if (finished?.axis !== 'x') return;
      const speed = swipeSpeed(finished.moves);
      const opens = speed > FLICK_SPEED || (speed >= -FLICK_SPEED && shown.current > 0.5);
      shown.target = opens ? 1 : 0;
      if (opens) openLayer('menuOpen');
      else closeLayer('menuOpen');
    }

    addEventListener('touchstart', ontouchstart, { passive: true });
    addEventListener('touchmove', ontouchmove, { passive: false });
    addEventListener('touchend', ontouchend);
    addEventListener('touchcancel', ontouchend);
    return () => {
      removeEventListener('touchstart', ontouchstart);
      removeEventListener('touchmove', ontouchmove);
      removeEventListener('touchend', ontouchend);
      removeEventListener('touchcancel', ontouchend);
    };
  });
</script>

{#if shown.current > 0}
  <button
    class="fixed inset-0 z-(--z-nav-backdrop) cursor-default bg-black"
    style:opacity={0.5 * shown.current}
    aria-label="Close the menu"
    onclick={() => closeLayer('menuOpen')}
  ></button>
{/if}
<nav
  class={['fixed inset-y-0 left-0 z-(--z-nav) flex w-[min(80vw,300px)] flex-col bg-base-200 shadow-xl', shown.current === 0 && 'invisible']}
  style:transform="translateX({(shown.current - 1) * 100}%)"
  bind:clientWidth={width}
  aria-label="Pages"
>
  <p class="px-5 pt-5 pb-3 text-lg font-semibold">Foryooru</p>
  <ul class="menu w-full gap-1 text-base">
    {#each TABS as tab (tab.path)}
      <li>
        <a href={tabHref(tab)} class={[appPath() === tab.path && 'menu-active']} onclick={(e) => pick(e, tab)}>
          <TabIcon {tab} class="size-5" />{tab.label}
        </a>
      </li>
    {/each}
  </ul>
</nav>
