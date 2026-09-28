// Panel de memoria de un proceso (sección 9): frames, globales, heap y flechas de punteros.
import type { Arrow, MemoryLayout, Prim } from '../layout/memoryLayout.ts';
import { CELL_H } from '../layout/constants.ts';
import { tipProps, useScene } from '../SceneContext.tsx';

function arrowPath(a: Arrow): string {
  const { from } = a;
  if (!a.to) return `M${from.x},${from.y} C${from.x + 30},${from.y} ${from.x + 34},${from.y + 14} ${from.x + 52},${from.y + 14}`;
  const r = a.to;
  const ty = r.y + Math.min(r.h / 2, CELL_H / 2 + 2);
  if (a.side === 'left') {
    const ex = r.x - 2;
    const dx = Math.max(30, (ex - from.x) / 2);
    return `M${from.x},${from.y} C${from.x + dx},${from.y} ${ex - dx},${ty} ${ex},${ty}`;
  }
  const ex = r.x + r.w + 2;
  const bulge = Math.max(from.x, ex) + 34 + Math.abs(from.y - ty) * 0.15;
  return `M${from.x},${from.y} C${bulge},${from.y} ${bulge},${ty} ${ex},${ty}`;
}

export function MemoryPanel(props: {
  layout: MemoryLayout;
  prevValues: Map<string, string> | null;
  x: number;
  y: number;
  flash: boolean;
}) {
  const { layout, prevValues, x, y, flash } = props;
  const ctx = useScene();
  const changed = (key: string) => {
    if (!prevValues) return false;
    const now = layout.values.get(key);
    return prevValues.get(key) !== now;
  };

  const render = (p: Prim, i: number) => {
    switch (p.k) {
      case 'panel': {
        const cls = `mem-panel ${p.tone}`;
        return (
          <g key={i}>
            <rect x={p.r.x} y={p.r.y} width={p.r.w} height={p.r.h} rx={10} className={cls} />
            {p.tone !== 'globals' && <rect x={p.r.x} y={p.r.y + 8} width={4} height={p.r.h - 16} rx={2} style={{ fill: p.ink ?? 'var(--purple)' }} />}
            <text x={p.r.x + 12} y={p.r.y + 16} className="mem-title">
              {p.title}
              {p.sub && <tspan className="mem-sub"> {p.sub}</tspan>}
            </text>
          </g>
        );
      }
      case 'heap':
        return (
          <g key={i} {...tipProps(ctx, { title: `Bloque del heap ${p.addr}`, body: `${p.sub}${p.freed ? ' · liberado con free' : ''}${p.leak ? ' · nunca se liberó (fuga)' : ''}` })}>
            <rect x={p.r.x} y={p.r.y} width={p.r.w} height={p.r.h} rx={10} className={`mem-heap${p.freed ? ' freed' : ''}${p.leak ? ' leak' : ''}`} />
            <text x={p.r.x + 10} y={p.r.y + 16} className="mem-title">
              {p.title}
              <tspan className="mem-sub"> {p.sub}</tspan>
            </text>
            {p.freed && (
              <>
                <rect x={p.r.x} y={p.r.y} width={p.r.w} height={p.r.h} rx={10} fill="url(#hatch-freed)" />
                <text x={p.r.x + p.r.w - 8} y={p.r.y + p.r.h - 7} className="mem-chip freed" textAnchor="end">
                  liberado
                </text>
              </>
            )}
            {p.leak && (
              <text x={p.r.x + p.r.w - 8} y={p.r.y + p.r.h - 7} className="mem-chip leak" textAnchor="end">
                fuga
              </text>
            )}
          </g>
        );
      case 'name':
        return (
          <text key={i} x={p.x} y={p.y + 4} className="mem-name" {...tipProps(ctx, { title: `${p.text}: ${p.type}`, body: `Dirección ${p.addr}` })}>
            {p.text}
          </text>
        );
      case 'cell':
        return (
          <g key={i} {...tipProps(ctx, { title: p.uninit ? 'Sin inicializar' : p.text, body: p.tip })}>
            <rect x={p.r.x} y={p.r.y} width={p.r.w} height={p.r.h} rx={4} className={`mem-cell${p.uninit ? ' uninit' : ''}${changed(p.key) ? ' changed' : ''}`} />
            <text x={p.r.x + 6} y={p.r.y + p.r.h / 2 + 4} className={`mem-value${p.uninit ? ' uninit' : ''}${p.italic ? ' italic' : ''}`}>
              {p.text}
            </text>
          </g>
        );
      case 'ptr': {
        const cx = p.r.x + p.r.w / 2;
        const cy = p.r.y + p.r.h / 2;
        return (
          <g key={i} {...tipProps(ctx, { title: p.uninit ? 'Puntero sin inicializar' : p.target === null ? 'NULL' : `Apunta a ${p.target}`, body: p.tip })}>
            <rect x={p.r.x} y={p.r.y} width={p.r.w} height={p.r.h} rx={4} className={`mem-cell${p.uninit ? ' uninit' : ''}${changed(p.key) ? ' changed' : ''}`} />
            {p.uninit ? (
              <text x={cx} y={cy + 4} className="mem-value uninit" textAnchor="middle">
                ?
              </text>
            ) : p.target === null ? (
              <g className="mem-ground">
                <line x1={cx} y1={cy - 6} x2={cx} y2={cy} />
                <line x1={cx - 7} y1={cy} x2={cx + 7} y2={cy} />
                <line x1={cx - 4.5} y1={cy + 3} x2={cx + 4.5} y2={cy + 3} />
                <line x1={cx - 2} y1={cy + 6} x2={cx + 2} y2={cy + 6} />
              </g>
            ) : (
              <circle cx={cx} cy={cy} r={3.5} className="mem-dot" />
            )}
          </g>
        );
      }
      case 'index':
        return (
          <text key={i} x={p.x} y={p.y} className="mem-index" textAnchor="middle">
            {p.text}
          </text>
        );
      case 'box':
        return <rect key={i} x={p.r.x} y={p.r.y} width={p.r.w} height={p.r.h} rx={6} className="mem-box" />;
      case 'label':
        return (
          <text key={i} x={p.x} y={p.y + 4} className="mem-label">
            {p.text}
          </text>
        );
    }
  };

  return (
    <g transform={`translate(${x},${y})`} className="memory">
      {layout.prims.map(render)}
      {layout.arrows.map((a, i) => {
        const dangling = !a.to;
        const danger = dangling || a.freed;
        return (
          <g key={`arrow-${i}`} className={`mem-arrow${danger ? ' danger' : ''}`}>
            <path d={arrowPath(a)} markerEnd={dangling ? undefined : danger ? 'url(#arrow-danger)' : 'url(#arrow)'} />
            {dangling && (
              <g className="mem-cut">
                <line x1={a.from.x + 54} y1={a.from.y + 7} x2={a.from.x + 60} y2={a.from.y + 21} />
                <line x1={a.from.x + 60} y1={a.from.y + 7} x2={a.from.x + 66} y2={a.from.y + 21} />
              </g>
            )}
          </g>
        );
      })}
      {flash && <rect x={-6} y={-6} width={layout.w + 12} height={layout.h + 12} rx={10} className="mem-flash" />}
    </g>
  );
}
