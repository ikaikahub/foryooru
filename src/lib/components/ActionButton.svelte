<script lang="ts" module>
  export const CANCELLED = Symbol('cancelled');
</script>

<script lang="ts">
  import { errorMessage } from '$lib/api';
  import type { ClassValue } from 'svelte/elements';

  let {
    icon,
    label,
    doneLabel,
    action,
    iconEnd = false,
    class: className,
    title,
  }: {
    icon: string;
    label: string;
    doneLabel: string;
    action: () => Promise<unknown>;
    iconEnd?: boolean;
    class?: ClassValue;
    title?: string;
  } = $props();

  const SHOW_RESULT_MS = 1500;

  let result = $state<{ icon: string; label: string; title?: string }>();

  async function run() {
    try {
      if ((await action()) === CANCELLED) return;
      result = { icon: '✅', label: doneLabel };
    } catch (error) {
      result = { icon: '⚠️', label: 'Failed', title: errorMessage(error) };
    }
    setTimeout(() => (result = undefined), SHOW_RESULT_MS);
  }

  const shownIcon = $derived(result?.icon ?? icon);
</script>

<button type="button" class={className} title={result?.title ?? title} onclick={run}>
  {#if !iconEnd}<span>{shownIcon}</span>{/if}
  <span>{result?.label ?? label}</span>
  {#if iconEnd}<span>{shownIcon}</span>{/if}
</button>
