// Medidas del lienzo en unidades SVG (1 unidad = 1 px con zoom 1).

// Tipografía monoespaciada de la memoria: JetBrains Mono avanza 0,6 em por carácter.
export const MEM_FONT = 11;
export const CW = MEM_FONT * 0.6;
export const ROW_H = 22;
export const CELL_H = 18;
export const INDEX_H = 12;

// Cuadrado de proceso
export const BOX_PAD = 14;
export const HEADER_H = 46;
export const LANE_H = 36;
export const LANES_PAD_TOP = 20;
export const LANES_PAD_BOTTOM = 10;
export const WINDOW = 12; // pasos visibles en los carriles
export const COL_W = 26;
export const LABEL_W = 78;
export const NOW_W = 190;
export const MEM_TITLE_H = 28;
export const CONSOLE_LINES = 2;
export const CONSOLE_H = 30 + CONSOLE_LINES * 15;
export const MIN_BOX_W = BOX_PAD * 2 + LABEL_W + WINDOW * COL_W + NOW_W;

export const BLACKBOX_H = 74;
export const INIT_W = 116;
export const INIT_H = 40;

export const REAPED_W = 170;
export const REAPED_H = 40;

// Árbol de procesos: el espacio horizontal alcanza para un tubo entre hermanos.
export const GAP_X = 170;
export const GAP_Y = 90;

// Puertos y tubos
export const PORT_W = 24;
export const PORT_H = 16;
export const PORT_GAP = 22;
export const TUBE_LEN = 130;
export const TUBE_R = 15;
