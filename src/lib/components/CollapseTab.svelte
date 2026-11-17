<script lang="ts">
  import type { HTMLButtonAttributes } from 'svelte/elements';

  let { class: className, children, ...rest }: HTMLButtonAttributes = $props();
</script>

<button
  class={[ //top-1/2 -translate-y-1/2 idkk
    'absolute top-6 left-full z-20 cursor-pointer rounded-r-box border border-l-0 border-base-300 bg-base-200 px-1 py-2 text-xs leading-none hover:bg-base-300',
    className,
  ]}
  {...rest}
>
  {@render children?.()}
</button>
