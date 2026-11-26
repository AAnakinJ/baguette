// Monochrome mark that follows the theme (currentColor): a conductor's baton.
export function Logo({ size = 22 }: { size?: number }) {
  return (
    <svg className="logo" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
      <rect x="1" y="1" width="22" height="22" rx="5" fill="none" stroke="currentColor" strokeWidth="1.6" />
      <path d="M7.5 16.5 L16.5 7.5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
      <circle cx="7" cy="17" r="2.1" fill="currentColor" />
    </svg>
  );
}

/** Crescent moon for the night pilot (inherits the text colour and size). */
export function Moon({ size = 14 }: { size?: number }) {
  return (
    <svg className="icon-inline" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
      <path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5Z" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" />
    </svg>
  );
}
