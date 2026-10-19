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

export const MapIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3.5" y="4" width="17" height="16" rx="2" />
    <path d="M11 4v16M11 12h9.5M16 12v8" />
  </Svg>
);

export const DiskIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3.5" y="6" width="17" height="12" rx="2.2" />
    <path d="M3.5 13.5h17" />
    <path d="M16.5 16h.01M13.5 16h.01" />
  </Svg>
);

export const PulseIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M20.2 12.6A5 5 0 0 0 12 6.6a5 5 0 0 0-8.2 6l8.2 7.7 3-2.8" />
    <path d="M14 13h2l1.2-2.5 1.6 5 1.2-2.5h2" />
  </Svg>
);

export const GlobeIcon = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="8.5" />
    <path d="M3.5 12h17M12 3.5c2.4 2.4 3.6 5.2 3.6 8.5s-1.2 6.1-3.6 8.5c-2.4-2.4-3.6-5.2-3.6-8.5s1.2-6.1 3.6-8.5Z" />
  </Svg>
);

export const PrinterIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M7 9V4h10v5" />
    <rect x="3.5" y="9" width="17" height="8" rx="2" />
    <path d="M7 14h10v6H7z" />
  </Svg>
);

export const AppsIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="4" y="4" width="7" height="7" rx="1.6" />
    <rect x="13" y="4" width="7" height="7" rx="1.6" />
    <rect x="4" y="13" width="7" height="7" rx="1.6" />
    <rect x="13" y="13" width="7" height="7" rx="1.6" />
  </Svg>
);

export const WindowIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3.5" y="4.5" width="17" height="15" rx="2" />
    <path d="M3.5 8.5h17" />
  </Svg>
);

export const LockIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="5" y="10.5" width="14" height="10" rx="2" />
    <path d="M8 10.5V8a4 4 0 0 1 8 0v2.5" />
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

export const FolderIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M3 7.5a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" />
  </Svg>
);

export const CheckCircleIcon = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="8.5" />
    <path d="m8.3 12.2 2.5 2.5 5-5" />
  </Svg>
);

// Remedies library categories (the `icon` name in remedies/*.json).
const CATEGORY_PATHS: Record<string, string[]> = {
  wifi: ["M2.5 9a14 14 0 0 1 19 0", "M5.5 12.3a9.5 9.5 0 0 1 13 0", "M8.6 15.6a5 5 0 0 1 6.8 0", "M12 19.2h.01"],
  mail: ["M3.5 6.5h17v11h-17z", "m3.8 7 8.2 6.2L20.2 7"],
  video: ["M3.5 7h11v10h-11z", "m14.5 10.5 6-3.5v10l-6-3.5"],
  sound: ["M4 9.5h3.5L12 5.5v13l-4.5-4H4z", "M15.5 9a4 4 0 0 1 0 6", "M18 6.5a7.5 7.5 0 0 1 0 11"],
  camera: ["M3.5 8h4l1.5-2.5h6L16.5 8h4v11h-17z", "M12 16.5a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4z"],
  monitor: ["M3.5 4.5h17v11.5h-17z", "M8.5 20h7", "M12 16v4"],
  bluetooth: ["m7 7.5 10 9-5 4v-17l5 4-10 9"],
  keyboard: ["M2.5 6.5h19v11h-19z", "M6 10h.01M9.5 10h.01M13 10h.01M16.5 10h.01M7.5 14h9"],
  power: ["M12 3.5v8", "M7 6.2a7.5 7.5 0 1 0 10 0"],
  update: ["M19.5 12a7.5 7.5 0 0 1-13 5.1", "M4.5 12a7.5 7.5 0 0 1 13-5.1", "M17.5 3.5v3.4h-3.4", "M6.5 20.5v-3.4h3.4"],
  battery: ["M3 7.5h15.5v9H3z", "M21 10.5v3", "M6 10.5v3M9 10.5v3"],
  remote: ["M3.5 4.5h17v11h-17z", "M8.5 20h7", "m10 8 3 2-3 2"],
  clock: ["M12 20.5a8.5 8.5 0 1 0 0-17 8.5 8.5 0 0 0 0 17z", "M12 7.5V12l3 2"],
  phone: ["M7.5 2.5h9v19h-9z", "M11 18.5h2"],
  backup: ["M12 20.5a8.5 8.5 0 1 1 8.2-6.3", "M20.5 9.5v4.7h-4.7", "M12 7.5V12l3 2"],
  eye: ["M2.5 12s3.5-6.5 9.5-6.5 9.5 6.5 9.5 6.5-3.5 6.5-9.5 6.5S2.5 12 2.5 12z", "M12 14.8a2.8 2.8 0 1 0 0-5.6 2.8 2.8 0 0 0 0 5.6z"],
};

export function CategoryIcon({ name, size = 18 }: { name: string; size?: number }) {
  const known: Record<string, (p: IconProps) => React.ReactElement> = {
    globe: GlobeIcon,
    printer: PrinterIcon,
    apps: AppsIcon,
    pulse: PulseIcon,
    disk: DiskIcon,
    folder: FolderIcon,
    lock: LockIcon,
    shield: ShieldIcon,
    window: WindowIcon,
    wrench: WrenchIcon,
  };
  const Known = known[name];
  if (Known) return <Known size={size} />;
  return (
    <Svg size={size}>
      {(CATEGORY_PATHS[name] ?? CATEGORY_PATHS.update).map((d) => (
        <path key={d} d={d} />
      ))}
    </Svg>
  );
}
