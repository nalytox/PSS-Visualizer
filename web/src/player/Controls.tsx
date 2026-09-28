// Controles de ejecución (sección 10).
import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, Pause, Play, SkipBack, SkipForward, Footprints } from 'lucide-react';
import { threadLabel, type TraceIndex } from '../trace/query.ts';
import { SPEEDS, type Player } from './usePlayer.ts';

export function Controls({ player, index }: { player: Player; index: TraceIndex }) {
  const atStart = player.t === 0;
  const atEnd = player.t === player.last;
  return (
    <div className="controls" role="toolbar" aria-label="Controles de ejecución">
      <div className="btn-group">
        <button type="button" onClick={player.first} disabled={atStart} title="Primer paso (Inicio)" aria-label="Primer paso">
          <SkipBack size={18} />
        </button>
        <button type="button" onClick={player.prevEvent} disabled={atStart} title="Evento anterior (Shift + ←)" aria-label="Evento anterior">
          <ChevronsLeft size={18} />
        </button>
        <button type="button" onClick={player.prev} disabled={atStart} title="Paso atrás (←)" aria-label="Paso atrás">
          <ChevronLeft size={20} />
        </button>
        <button type="button" className="primary play" onClick={player.togglePlay} title="Reproducir o pausar (Espacio)" aria-label={player.playing ? 'Pausar' : 'Reproducir'}>
          {player.playing ? <Pause size={20} /> : <Play size={20} />}
        </button>
        <button type="button" onClick={player.next} disabled={atEnd} title="Paso adelante (→)" aria-label="Paso adelante">
          <ChevronRight size={20} />
        </button>
        <button type="button" onClick={player.nextEvent} disabled={atEnd} title="Siguiente evento (Shift + →)" aria-label="Siguiente evento">
          <ChevronsRight size={18} />
        </button>
        <button type="button" onClick={player.lastStep} disabled={atEnd} title="Último paso (Fin)" aria-label="Último paso">
          <SkipForward size={18} />
        </button>
      </div>
      <button
        type="button"
        className="thread-step"
        onClick={player.threadStep}
        disabled={atEnd}
        title="Avanza hasta el próximo paso del hilo seleccionado (Alt + →)"
        aria-label="Paso del hilo seleccionado"
      >
        <Footprints size={16} />
        <span>{player.selectedTid !== null ? `Paso de ${threadLabel(index, player.selectedTid)} (${player.selectedTid})` : 'Paso del hilo'}</span>
      </button>
      <label className="speed">
        <span>Velocidad</span>
        <select value={player.speed} onChange={(e) => player.setSpeed(Number(e.target.value))}>
          {SPEEDS.map((s) => (
            <option key={s} value={s}>
              {String(s).replace('.', ',')}x
            </option>
          ))}
        </select>
      </label>
      <div className="step-counter" aria-live="off">
        Paso <strong>{player.t}</strong> de {player.last}
      </div>
    </div>
  );
}
