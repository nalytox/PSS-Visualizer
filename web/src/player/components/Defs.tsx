// Patrones, marcadores y filtros compartidos por todo el lienzo.
export function Defs() {
  return (
    <defs>
      <pattern id="hatch-blocked" width="8" height="8" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
        <rect width="8" height="8" style={{ fill: 'var(--blocked-soft)' }} />
        <line x1="0" y1="0" x2="0" y2="8" style={{ stroke: 'var(--blocked)', strokeWidth: 3 }} />
      </pattern>
      <pattern id="hatch-freed" width="7" height="7" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
        <line x1="0" y1="0" x2="0" y2="7" style={{ stroke: 'var(--text-2)', strokeWidth: 1.2, opacity: 0.45 }} />
      </pattern>
      <linearGradient id="glass" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stopColor="#ffffff" stopOpacity="0.85" />
        <stop offset="0.45" stopColor="#ffffff" stopOpacity="0.25" />
        <stop offset="1" stopColor="#d9ccfa" stopOpacity="0.35" />
      </linearGradient>
      <filter id="soft-shadow" x="-10%" y="-10%" width="120%" height="130%">
        <feDropShadow dx="0" dy="5" stdDeviation="8" floodColor="#5b4a8a" floodOpacity="0.14" />
      </filter>
      <filter id="glow" x="-20%" y="-20%" width="140%" height="140%">
        <feDropShadow dx="0" dy="0" stdDeviation="6" floodColor="#9b7fd9" floodOpacity="0.45" />
      </filter>
      <marker id="arrow" viewBox="0 0 10 10" refX="8.5" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
        <path d="M0,1 L9,5 L0,9 z" style={{ fill: 'var(--text)' }} />
      </marker>
      <marker id="arrow-danger" viewBox="0 0 10 10" refX="8.5" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
        <path d="M0,1 L9,5 L0,9 z" style={{ fill: 'var(--danger)' }} />
      </marker>
      <marker id="arrow-purple" viewBox="0 0 10 10" refX="8.5" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
        <path d="M0,1 L9,5 L0,9 z" style={{ fill: 'var(--purple)' }} />
      </marker>
    </defs>
  );
}
