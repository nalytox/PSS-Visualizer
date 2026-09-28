//! Bloques pedidos por el programa con malloc y compañía. Las asignaciones internas de libc (por
//! ejemplo el buffer de stdout) no pasan por aquí: solo se registran las llamadas del usuario.

use trace_model::FreeError;

/// Cuántos pasos sigue visible un bloque liberado (rayado) antes de retirarse del dibujo.
pub const FREED_VISIBLE_STEPS: u64 = 3;

#[derive(Debug, Clone)]
pub struct Block {
    pub addr: u64,
    pub size: u64,
    pub alloc_at: u64,
    pub alloc_line: u32,
    pub freed_at: Option<u64>,
    /// Contenido al momento del free: después libc escribe ahí sus propios datos (con una clave
    /// aleatoria por proceso), así que el bloque liberado se muestra con lo que tenía.
    pub last_contents: Option<Vec<u8>>,
}

#[derive(Default, Clone)]
pub struct Heap {
    pub blocks: Vec<Block>,
}

impl Heap {
    pub fn alloc(&mut self, addr: u64, size: u64, t: u64, line: u32) {
        // Un bloque liberado puede volver a entregarse en la misma dirección.
        self.blocks.retain(|b| b.addr != addr);
        self.blocks.push(Block {
            addr,
            size,
            alloc_at: t,
            alloc_line: line,
            freed_at: None,
            last_contents: None,
        });
    }

    pub fn free(&mut self, addr: u64, t: u64) -> Result<(), FreeError> {
        match self.blocks.iter_mut().find(|b| b.addr == addr) {
            Some(b) if b.freed_at.is_none() => {
                b.freed_at = Some(t);
                Ok(())
            }
            Some(_) => Err(FreeError::DoubleFree),
            None => Err(FreeError::InvalidPointer),
        }
    }

    /// Guarda el contenido de un bloque vivo justo antes de liberarlo.
    pub fn remember(&mut self, addr: u64, read: impl FnOnce(u64) -> Option<Vec<u8>>) {
        if let Some(b) = self.blocks.iter_mut().find(|b| b.addr == addr && b.freed_at.is_none()) {
            b.last_contents = read(b.size);
        }
    }

    /// Bytes de un bloque liberado que cubren [addr, addr + len), si los hay.
    pub fn freed_bytes(&self, addr: u64, len: usize) -> Option<&[u8]> {
        self.blocks.iter().find_map(|b| {
            let c = b.last_contents.as_ref().filter(|_| b.freed_at.is_some())?;
            let off = addr.checked_sub(b.addr)? as usize;
            c.get(off..off + len)
        })
    }

    pub fn live(&self) -> impl Iterator<Item = &Block> {
        self.blocks.iter().filter(|b| b.freed_at.is_none())
    }

    pub fn visible(&self, t: u64) -> impl Iterator<Item = &Block> {
        self.blocks
            .iter()
            .filter(move |b| b.freed_at.is_none_or(|f| t - f <= FREED_VISIBLE_STEPS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_double_and_invalid_free() {
        let mut h = Heap::default();
        h.alloc(0x1000, 16, 1, 5);
        assert!(h.free(0x1000, 2).is_ok());
        assert_eq!(h.free(0x1000, 3), Err(FreeError::DoubleFree));
        assert_eq!(h.free(0x2000, 3), Err(FreeError::InvalidPointer));
        h.alloc(0x1000, 32, 4, 6);
        assert_eq!(h.live().count(), 1);
    }

    #[test]
    fn freed_blocks_stay_visible_for_a_few_steps() {
        let mut h = Heap::default();
        h.alloc(0x1000, 16, 1, 5);
        h.free(0x1000, 2).unwrap();
        assert_eq!(h.visible(2 + FREED_VISIBLE_STEPS).count(), 1);
        assert_eq!(h.visible(3 + FREED_VISIBLE_STEPS).count(), 0);
    }
}
