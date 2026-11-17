addEventListener('keyboardinset', (event) => {
    const inset = (event as CustomEvent<number>).detail;
    document.documentElement.style.setProperty('--keyboard-inset', `${inset}px`);
});
