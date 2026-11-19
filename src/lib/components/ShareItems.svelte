<script lang="ts">
  import type { Post } from '$lib/api';
  import { copyImage, copyImageLink, copyPostLink, openPostLink, saveOriginal, saveSample } from '$lib/postActions';
  import type { ClassValue } from 'svelte/elements';
  import ActionButton from './ActionButton.svelte';

  let { post, class: className, iconEnd = false }: { post: Post; class?: ClassValue; iconEnd?: boolean } = $props();
</script>
<li><ActionButton class={className} {iconEnd} icon="↗" label="Open original url" doneLabel="Copied" action={() => openPostLink(post)} /></li>
<li><ActionButton class={className} {iconEnd} icon="🌐" label="Copy post address" doneLabel="Copied" action={() => copyPostLink(post)} /></li>
<li><ActionButton class={className} {iconEnd} icon="🔗" label="Copy media address" doneLabel="Copied" action={() => copyImageLink(post)} /></li>
{#if post.media !== 'video'}
  <li><ActionButton class={className} {iconEnd} icon="📋" label="Copy image" doneLabel="Copied" action={() => copyImage(post)} /></li>
{/if}
{#if post.media === 'image'}
  <li><ActionButton class={className} {iconEnd} icon="📥" label="Download sample" doneLabel="Downloaded" action={() => saveSample(post)} /></li>
{/if}
<li><ActionButton class={className} {iconEnd} icon="💾" label="Download original" doneLabel="Downloaded" action={() => saveOriginal(post)} /></li>
