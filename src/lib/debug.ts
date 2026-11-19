import type { TimingSpan } from './api';

function formatDuration(ms: number) {
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)}s` : `${Math.round(ms)}ms`;
}

export function debugReport(spans: TimingSpan[], site: string) {
  const header = [
    `foryooru debug ${new Date().toISOString()}`,
    `site ${site}`,
    navigator.userAgent,
    `${spans.length} spans`,
  ];
  const rows = spans.map((span) =>
    `${span.at.toFixed(2).padStart(8)}s  ${span.name.padEnd(14)} ${formatDuration(span.ms).padStart(7)}  ${span.note}`.trimEnd(),
  );
  return [...header, '', ...rows].join('\n');
}
