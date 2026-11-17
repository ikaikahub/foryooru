<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    icon,
    iconImage,
    label,
    detail,
    onclick,
    children,
  }: { icon?: string; iconImage?: string; label: string; detail?: string; onclick?: () => void; children?: Snippet } = $props();

  const ROW = 'flex w-full flex-wrap items-center gap-x-3 gap-y-2 px-3 py-2.5 text-left';
</script>

{#snippet content()}
  {#if iconImage || icon}
    <span class="flex w-6 shrink-0 justify-center text-lg">
      {#if iconImage}<img class="size-[1.125rem]" src={iconImage} alt="" />{:else}{icon}{/if}
    </span>
  {/if}
  <div class="min-w-28 flex-1">
    <div>{label}</div>
    {#if detail}<div class="text-xs opacity-60">{detail}</div>{/if}
  </div>
  {#if children}<div class="flex max-w-full items-center gap-2">{@render children()}</div>{/if}
{/snippet}

<li>
  {#if onclick}
    <button class={[ROW, 'cursor-pointer active:bg-base-300']} {onclick}>
      {@render content()}
      <span class="text-xl leading-none opacity-40">›</span>
    </button>
  {:else}
    <div class={ROW}>{@render content()}</div>
  {/if}
</li>
