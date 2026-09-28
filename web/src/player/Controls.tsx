// Controles de ejecución (sección 10).
import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, Pause, Play, SkipBack, SkipForward, Footprints, Zap, Dices } from 'lucide-react';
import { threadLabel, type TraceIndex } from '../trace/query.ts';
import { SPEEDS, type Player } from './usePlayer.ts';

type PolicyName = 'round_robin' | 'random' | 'manual';

export function Controls(props: {
  player: Player;
  index: TraceIndex;
  onCtrlC?: () => void;
  policy: PolicyName;
  seed: number;
  onPolicy?: (policy: PolicyName, seed?: number) => void;
}) {
  const { player, index, onCtrlC, policy, seed, onPolicy } = props;
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
      <button
        type="button"
        className="ctrl-c"
        onClick={onCtrlC}
        disabled={!onCtrlC || atEnd}
        title={onCtrlC ? 'Envía SIGINT al programa después de este paso y vuelve a ejecutarlo desde aquí' : 'Ctrl+C necesita el servidor local'}
        aria-label="Enviar Ctrl+C"
      >
        <Zap size={16} />
        <span>Ctrl+C</span>
      </button>
      <label className="speed policy" title={onPolicy ? 'Quién avanza en cada paso' : 'Cambiar la planificación necesita el servidor local'}>
        <span>Planificación</span>
        <select value={policy} disabled={!onPolicy} onChange={(e) => onPolicy?.(e.target.value as PolicyName)}>
          <option value="round_robin">Round-robin</option>
          <option value="random">Aleatoria</option>
          <option value="manual">Manual</option>
        </select>
      </label>
      {policy === 'random' && (
        <button type="button" className="thread-step" disabled={!onPolicy} onClick={() => onPolicy?.('random')} title="Vuelve a ejecutar con otra semilla" aria-label="Otra semilla">
          <Dices size={16} />
          <span>semilla {seed}</span>
        </button>
      )}
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
