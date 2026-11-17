<script lang="ts">
  import type { Snippet } from 'svelte';
  import { tick } from 'svelte';
  import type { ClassValue } from 'svelte/elements';

  type Anchor = { getBoundingClientRect(): DOMRect };

  let {
    anchor,
    placement = 'below',
    class: className = 'menu-sm w-56',
    onclose,
    children,
  }: {
    anchor: Anchor | null;
    placement?: 'below' | 'above' | 'left' | 'right';
    class?: ClassValue;
    onclose: () => void;
    children: Snippet;
  } = $props();

  const GAP = 4;
  const EDGE = 8;

  let menu = $state<HTMLElement>();
  let left = $state(0);
  let top = $state(0);

  const clamp = (value: number, size: number, room: number) => Math.max(EDGE, Math.min(value, room - size - EDGE));

  $effect(() => {
    if (!menu || !anchor) return;
    const [element, from] = [menu, anchor];
    element.showPopover();
    tick().then(() => {
      const rect = from.getBoundingClientRect();
      const [width, height] = [element.offsetWidth, element.offsetHeight];
      if (placement === 'left') {
        left = clamp(rect.left - width - GAP, width, window.innerWidth);
        top = clamp(rect.bottom - height, height, window.innerHeight);
      } else if (placement === 'right') {
        left = clamp(rect.right + GAP, width, window.innerWidth);
        top = clamp(rect.top, height, window.innerHeight);
      } else if (placement === 'above') {
        left = clamp(rect.right - width, width, window.innerWidth);
        top = clamp(rect.top - height - GAP, height, window.innerHeight);
      } else {
        const fitsBelow = rect.bottom + GAP + height < window.innerHeight;
        top = fitsBelow ? rect.bottom + GAP : clamp(rect.top - height - GAP, height, window.innerHeight);
        left = clamp(rect.left, width, window.innerWidth);
      }
    });
    return () => element.hidePopover();
  });

  function onkeydown(e: KeyboardEvent) {
    if (!anchor || e.key !== 'Escape') return;
    e.stopImmediatePropagation();
    onclose();
  }
</script>

<svelte:window onkeydowncapture={onkeydown} />

{#if anchor}
  <div
    class="fixed inset-0 z-(--z-menu-backdrop)"
    role="presentation"
    onclick={onclose}
    oncontextmenu={(e) => {
      e.preventDefault();
      onclose();
    }}
  ></div>
{/if}

<ul
  bind:this={menu}
  popover="manual"
  class={['menu fixed m-0 rounded-box border border-base-300 bg-base-200 p-1 text-base-content shadow-lg', className]}
  style:left="{left}px"
  style:top="{top}px"
>
  {@render children()}
</ul>
