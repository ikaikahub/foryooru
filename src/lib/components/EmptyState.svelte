<script lang="ts">
  import PageMessage from './PageMessage.svelte';

  let { message }: { message: string } = $props();
</script>

<PageMessage><p class="opacity-70">{message}</p></PageMessage>
