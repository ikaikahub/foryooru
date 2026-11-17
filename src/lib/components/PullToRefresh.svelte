<script lang="ts">
  import { page } from '$app/state';
  import { SPRING, swipeAxis } from '$lib/motion';
  import { requestRerun } from '$lib/state/rerun.svelte';
  import { viewer } from '$lib/state/viewer.svelte';
  import { onMount } from 'svelte';
  import { Spring } from 'svelte/motion';
  import PullIndicator, { PULL_THRESHOLD, pullDistance } from './PullIndicator.svelte';

  const distance = new Spring(0, SPRING);

  onMount(() => {
    let pull: { x0: number; y0: number; mode: 'pull' | 'native' | null } | null = null;

    function ontouchstart(e: TouchEvent) {
      const blocked = e.touches.length !== 1 || scrollY > 0 || viewer.current !== null || page.state.searching || page.state.menuOpen;
      pull = blocked ? null : { x0: e.touches[0].clientX, y0: e.touches[0].clientY, mode: null };
    }

    function ontouchmove(e: TouchEvent) {
      if (!pull) return;
      const touch = e.touches[0];
      const [dx, dy] = [touch.clientX - pull.x0, touch.clientY - pull.y0];
      if (pull.mode === null) {
        const axis = swipeAxis(dx, dy);
        if (!axis) return;
        pull.mode = axis === 'y' && dy > 0 ? 'pull' : 'native';
      }
      if (pull.mode !== 'pull') return;
      e.preventDefault();
      distance.set(pullDistance(dy), { instant: true });
    }

    function ontouchend() {
      if (pull?.mode === 'pull' && distance.current >= PULL_THRESHOLD) requestRerun();
      pull = null;
      distance.target = 0;
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

<PullIndicator distance={distance.current} />
