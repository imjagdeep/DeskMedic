// One consistent line-icon set (24px grid, 1.8 stroke, currentColor), so
// every icon takes the colour of the text next to it.

type IconProps = { size?: number; className?: string };

function Svg({ size = 18, className, children }: IconProps & { children: React.ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export const OverviewIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M3.5 10.5 12 3.8l8.5 6.7" />
    <path d="M5.5 9v10.2a.8.8 0 0 0 .8.8H10v-5.5h4V20h3.7a.8.8 0 0 0 .8-.8V9" />
  </Svg>
);

export const MapIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3.5" y="4" width="17" height="16" rx="2" />
    <path d="M11 4v16M11 12h9.5M16 12v8" />
  </Svg>
);

export const BroomIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M19.5 4.5 12 12" />
    <path d="M12.8 11.2 9 9.5c-1.6 1.2-4 4.6-5 8.5l1.6 1.6c3.9-1 7.3-3.4 8.5-5Z" />
    <path d="m7 15 2 2" />
  </Svg>
);

export const DiskIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3.5" y="6" width="17" height="12" rx="2.2" />
    <path d="M3.5 13.5h17" />
    <path d="M16.5 16h.01M13.5 16h.01" />
  </Svg>
);

export const WrenchIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M14.7 6.3a4 4 0 0 0 5 5l-8.9 8.9a2 2 0 0 1-2.8-2.8l8.9-8.9a4 4 0 0 0-2.2-2.2Z" />
    <path d="M14.7 6.3a4 4 0 0 1 5.4-1.1l-2.6 2.6.3 2.2 2.2.3 2.6-2.6" />
  </Svg>
);

export const LogIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4.5 12a7.5 7.5 0 1 0 2.2-5.3L4.5 9" />
    <path d="M4.5 4.5V9H9" />
    <path d="M12 8v4l2.8 1.8" />
  </Svg>
);

export const SettingsIcon = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.6 1.6 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.6 1.6 0 0 0-1.8-.3 1.6 1.6 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.6 1.6 0 0 0-1-1.5 1.6 1.6 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.6 1.6 0 0 0 .3-1.8 1.6 1.6 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.6 1.6 0 0 0 1.5-1 1.6 1.6 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.6 1.6 0 0 0 1.8.3H9a1.6 1.6 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.6 1.6 0 0 0 1 1.5 1.6 1.6 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.6 1.6 0 0 0-.3 1.8V9a1.6 1.6 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.6 1.6 0 0 0-1.5 1Z" />
  </Svg>
);

export const ShieldIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 3.5 5 6.2v5.3c0 4.3 3 7.8 7 9 4-1.2 7-4.7 7-9V6.2Z" />
    <path d="m9 12 2.2 2.2L15.5 10" />
  </Svg>
);

export const AlertIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M10.3 4.2 2.9 17a2 2 0 0 0 1.7 3h14.8a2 2 0 0 0 1.7-3L13.7 4.2a2 2 0 0 0-3.4 0Z" />
    <path d="M12 9.5v4" />
    <path d="M12 16.8h.01" />
  </Svg>
);
