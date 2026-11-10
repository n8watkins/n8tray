// The window's own small glyphs (16 x 16, currentColor), decorative only:
// every button that shows one has its own accessible name.
import type { JSX } from "preact";

const paths: Record<string, JSX.Element> = {
  play: <path d="M4.5 2.8v10.4c0 .5.6.8 1 .5l8-5.2a.6.6 0 0 0 0-1l-8-5.2c-.4-.3-1 0-1 .5z" fill="currentColor" />,
  stop: <rect x="3.5" y="3.5" width="9" height="9" rx="1.6" fill="currentColor" />,
  restart: (
    <g fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="M13 8a5 5 0 1 1-1.5-3.6" />
      <path d="M13 2.5v3h-3" />
    </g>
  ),
  back: <path d="M10 3 5 8l5 5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />,
  close: <path d="m4 4 8 8m0-8-8 8" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />,
  gear: (
    <g fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
      <circle cx="8" cy="8" r="2.2" />
      <path d="M8 1.8v1.6M8 12.6v1.6M1.8 8h1.6M12.6 8h1.6M3.6 3.6l1.1 1.1M11.3 11.3l1.1 1.1M3.6 12.4l1.1-1.1M11.3 4.7l1.1-1.1" />
    </g>
  ),
  check: <path d="M3 8.5 6.5 12 13 4.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />,
  box: <rect x="3" y="3" width="10" height="10" rx="2" fill="none" stroke="currentColor" stroke-width="1.4" />,
  history: (
    <g fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
      <path d="M2 13.5h12" />
      <path d="M3.5 11 6.5 7l2.5 2.5L13 4" />
    </g>
  ),
  file: (
    <g fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round">
      <path d="M4 1.8h5l3 3v9.4H4z" />
      <path d="M9 1.8v3h3" />
    </g>
  ),
  folder: <path d="M1.8 4.2h4.4l1.4 1.5h6.6v7.5H1.8z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" />,
  web: (
    <g fill="none" stroke="currentColor" stroke-width="1.3">
      <circle cx="8" cy="8" r="6" />
      <path d="M2 8h12M8 2c2 2 2 10 0 12M8 2c-2 2-2 10 0 12" />
    </g>
  ),
  copy: (
    <g fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round">
      <rect x="5" y="5" width="8.5" height="8.5" rx="1.5" />
      <path d="M3 10.5V3.5c0-.8.7-1.5 1.5-1.5h6" />
    </g>
  ),
  info: (
    <g fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
      <circle cx="8" cy="8" r="6" />
      <path d="M8 7.2v4M8 4.8v.1" />
    </g>
  ),
  update: (
    <g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
      <path d="M8 2.5v8M4.8 7.4 8 10.6l3.2-3.2M3 13.5h10" />
    </g>
  ),
};

export type GlyphName = keyof typeof paths;

export function Glyph({ name, size = 14 }: { name: GlyphName; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      {paths[name]}
    </svg>
  );
}
