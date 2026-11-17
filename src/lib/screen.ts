import { MediaQuery } from 'svelte/reactivity';

export const drawerLayout = new MediaQuery('max-width: 700px');
export const android = /Android/.test(navigator.userAgent);
export const linuxDesktop = /Linux/.test(navigator.userAgent) && !android;
