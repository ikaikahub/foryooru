import type { TimingSpan } from './api';
import { BUILD_LABEL } from './build';

function formatDuration(ms: number) {
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)}s` : `${Math.round(ms)}ms`;
}

export function debugReport(spans: TimingSpan[]) {
  const header = [
    `foryooru debug ${new Date().toISOString()}`,
    `build ${BUILD_LABEL}`,
    navigator.userAgent,
    `${spans.length} spans`,
  ];
  const rows = spans.map((span) =>
    `${span.at.toFixed(2).padStart(8)}s  ${span.name.padEnd(14)} ${formatDuration(span.ms).padStart(7)}  ${span.note}`.trimEnd(),
  );
  return [...header, '', ...rows].join('\n');
}
