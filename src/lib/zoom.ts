import type { Attachment } from 'svelte/attachments';

const ZOOM_MAX = 6;
const ORIGINAL_AT = 1.5;
const GESTURE_GRACE_MS = 400;
const SNAP_BACK_BELOW = 1.15;

type Zoom = { scale: number; x: number; y: number };
type Point = { x: number; y: number };
type Gesture =
    | { kind: 'pinch'; start: Zoom; box: Point; distance: number; middle: Point }
    | { kind: 'pan'; start: Zoom; box: Point; from: Point };

const zooms = new WeakMap<HTMLImageElement, Zoom>();
let gestureEndedAt = 0;

export const isZoomed = (img: HTMLImageElement) => (zooms.get(img)?.scale ?? 1) > 1;
export const justZoomed = () => performance.now() - gestureEndedAt < GESTURE_GRACE_MS;

const point = (touch: Touch): Point => ({ x: touch.clientX, y: touch.clientY });

function apply(img: HTMLImageElement, zoom: Zoom, original: string | null) {
    const width = img.offsetWidth;
    const height = img.offsetHeight;
    zoom.scale = Math.min(ZOOM_MAX, zoom.scale);
    zoom.x = Math.min(0, Math.max(width - width * zoom.scale, zoom.x));
    zoom.y = Math.min(0, Math.max(height - height * zoom.scale, zoom.y));
    zooms.set(img, zoom);
    img.style.transform = zoom.scale > 1 ? `translate(${zoom.x}px, ${zoom.y}px) scale(${zoom.scale})` : '';
    if (zoom.scale > ORIGINAL_AT && original && img.src !== original) img.src = original;
}

export function resetZoom(img: HTMLImageElement | null | undefined) {
    if (img) apply(img, { scale: 1, x: 0, y: 0 }, null);
}

export function zoomable(original: string | null): Attachment<HTMLImageElement> {
    return (img) => {
        let gesture: Gesture | null = null;

        function begin(touches: TouchList) {
            const start = { ...(zooms.get(img) ?? { scale: 1, x: 0, y: 0 }) };
            const rect = img.getBoundingClientRect();
            const box = { x: rect.left - start.x, y: rect.top - start.y };
            if (touches.length >= 2) {
                const [a, b] = [point(touches[0]), point(touches[1])];
                gesture = { kind: 'pinch', start, box, distance: Math.hypot(a.x - b.x, a.y - b.y), middle: { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 } };
            } else if (start.scale > 1) {
                gesture = { kind: 'pan', start, box, from: point(touches[0]) };
            } else {
                gesture = null;
            }
        }

        function onstart(e: TouchEvent) {
            begin(e.touches);
        }

        function onmove(e: TouchEvent) {
            if (!gesture) return;
            e.preventDefault();
            e.stopPropagation();
            const { start, box } = gesture;
            if (gesture.kind === 'pinch' && e.touches.length >= 2) {
                const [a, b] = [point(e.touches[0]), point(e.touches[1])];
                const scale = Math.max(1, Math.min(ZOOM_MAX, (start.scale * Math.hypot(a.x - b.x, a.y - b.y)) / gesture.distance));
                const middle = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
                const u = (gesture.middle.x - box.x - start.x) / start.scale;
                const v = (gesture.middle.y - box.y - start.y) / start.scale;
                apply(img, { scale, x: middle.x - box.x - scale * u, y: middle.y - box.y - scale * v }, original);
            } else if (gesture.kind === 'pan') {
                const at = point(e.touches[0]);
                apply(img, { scale: start.scale, x: start.x + at.x - gesture.from.x, y: start.y + at.y - gesture.from.y }, original);
            }
        }

        function onend(e: TouchEvent) {
            if (!gesture) return;
            gestureEndedAt = performance.now();
            if ((zooms.get(img)?.scale ?? 1) < SNAP_BACK_BELOW) resetZoom(img);
            if (e.touches.length) begin(e.touches);
            else gesture = null;
        }

        img.classList.add('zoomable');
        img.addEventListener('touchstart', onstart, { passive: true });
        img.addEventListener('touchmove', onmove, { passive: false });
        img.addEventListener('touchend', onend);
        img.addEventListener('touchcancel', onend);
        return () => {
            img.removeEventListener('touchstart', onstart);
            img.removeEventListener('touchmove', onmove);
            img.removeEventListener('touchend', onend);
            img.removeEventListener('touchcancel', onend);
        };
    };
}
