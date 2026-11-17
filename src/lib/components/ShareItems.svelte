<script lang="ts">
  import type { Post } from '$lib/api';
  import { copyImage, copyImageLink, copyPostLink, openPostLink, saveOriginal, saveOriginalAs } from '$lib/postActions';
  import { android } from '$lib/screen';
  import type { ClassValue } from 'svelte/elements';
  import ActionButton from './ActionButton.svelte';

  let { post, class: className, iconEnd = false }: { post: Post; class?: ClassValue; iconEnd?: boolean } = $props();
</script>
<li><ActionButton class={className} {iconEnd} icon="↗" label="Open original url" doneLabel="Copied" action={() => openPostLink(post)} /></li>
<li><ActionButton class={className} {iconEnd} icon="🌐" label="Copy post address" doneLabel="Copied" action={() => copyPostLink(post)} /></li>
<li><ActionButton class={className} {iconEnd} icon="🔗" label="Copy image address" doneLabel="Copied" action={() => copyImageLink(post)} /></li>
{#if post.media !== 'video'}
  <li><ActionButton class={className} {iconEnd} icon="📋" label="Copy image" doneLabel="Copied" action={() => copyImage(post)} /></li>
{/if}
<li><ActionButton class={className} {iconEnd} icon="💾" label="Save image" doneLabel="Saved" action={() => saveOriginal(post)} /></li>
{#if !android}
  <li><ActionButton class={className} {iconEnd} icon="📂" label="Save image as…" doneLabel="Saved" action={() => saveOriginalAs(post)} /></li>
{/if}
