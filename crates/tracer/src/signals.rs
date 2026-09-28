//! Modelo de señales de cada proceso: acciones instaladas, máscara y señales pendientes. El kernel
//! hace la entrega real; este modelo sirve para dibujarla y para decidir si una señal despierta a
//! un proceso bloqueado.

use nix::sys::signal::Signal;
use std::collections::BTreeMap;
use trace_model::{InFlightSignal, ProcessSignals, SignalAction, SignalSource, SignalStatus};

#[derive(Clone, PartialEq, Debug)]
pub enum Action {
    Handler(String),
    Ignore,
}

#[derive(Clone, Debug)]
pub struct Pending {
    pub sig: i32,
    pub from: SignalSource,
}

#[derive(Clone, Default)]
pub struct SigState {
    pub actions: BTreeMap<i32, Action>,
    pub mask: u64,
    pub pending: Vec<Pending>,
}

pub fn name(sig: i32) -> String {
    Signal::try_from(sig)
        .map(|s| s.as_str().to_string())
        .unwrap_or_else(|_| format!("SIG{sig}"))
}

fn bit(sig: i32) -> u64 {
    if (1..=64).contains(&sig) { 1 << (sig - 1) } else { 0 }
}

/// Señales cuya acción por defecto es ignorarlas.
pub fn default_ignored(sig: i32) -> bool {
    matches!(sig, libc::SIGCHLD | libc::SIGURG | libc::SIGWINCH | libc::SIGCONT)
}

/// Señales de control de trabajos: detener un proceso no se modela todavía.
pub fn job_control(sig: i32) -> bool {
    matches!(sig, libc::SIGSTOP | libc::SIGTSTP | libc::SIGTTIN | libc::SIGTTOU)
}

/// Terminan al proceso y dejan un core.
pub fn dumps_core(sig: i32) -> bool {
    matches!(
        sig,
        libc::SIGSEGV
            | libc::SIGABRT
            | libc::SIGFPE
            | libc::SIGILL
            | libc::SIGBUS
            | libc::SIGQUIT
            | libc::SIGTRAP
            | libc::SIGSYS
    )
}

impl SigState {
    pub fn masked(&self, sig: i32) -> bool {
        self.mask & bit(sig) != 0
    }

    /// rt_sigprocmask: SIG_BLOCK, SIG_UNBLOCK o SIG_SETMASK. SIGKILL y SIGSTOP no se bloquean.
    pub fn set_mask(&mut self, how: i32, set: u64) {
        self.mask = match how {
            libc::SIG_BLOCK => self.mask | set,
            libc::SIG_UNBLOCK => self.mask & !set,
            _ => set,
        } & !(bit(libc::SIGKILL) | bit(libc::SIGSTOP));
    }

    pub fn set_action(&mut self, sig: i32, handler: u64, name: impl FnOnce(u64) -> String) {
        match handler {
            0 => {
                self.actions.remove(&sig);
            }
            1 => {
                self.actions.insert(sig, Action::Ignore);
            }
            addr => {
                self.actions.insert(sig, Action::Handler(name(addr)));
            }
        }
    }

    pub fn push(&mut self, sig: i32, from: SignalSource) {
        self.pending.push(Pending { sig, from });
    }

    /// La entrega de `sig` consume su entrada pendiente, si la había.
    pub fn take(&mut self, sig: i32) -> Option<Pending> {
        let k = self.pending.iter().position(|p| p.sig == sig)?;
        Some(self.pending.remove(k))
    }

    /// Hijo de fork: hereda acciones y máscara, no las pendientes.
    pub fn for_child(&self) -> Self {
        SigState {
            actions: self.actions.clone(),
            mask: self.mask,
            pending: Vec::new(),
        }
    }

    /// exec vuelve los handlers a la acción por defecto (el código del handler ya no existe); las
    /// señales ignoradas siguen ignoradas.
    pub fn exec(&mut self) {
        self.actions.retain(|_, a| *a == Action::Ignore);
    }

    /// ¿Hay una señal pendiente que interrumpiría una espera? `mask` reemplaza a la máscara del
    /// proceso durante sigsuspend.
    pub fn interrupts(&self, mask: Option<u64>) -> bool {
        let mask = mask.unwrap_or(self.mask);
        self.pending.iter().any(|p| {
            mask & bit(p.sig) == 0
                && !job_control(p.sig)
                && match self.actions.get(&p.sig) {
                    Some(Action::Ignore) => false,
                    Some(Action::Handler(_)) => true,
                    None => !default_ignored(p.sig),
                }
        })
    }

    pub fn view(&self) -> ProcessSignals {
        let names = |m: u64| (1..=64).filter(|s| m & bit(*s) != 0).map(name).collect();
        ProcessSignals {
            mask: names(self.mask),
            pending: self.pending.iter().map(|p| name(p.sig)).collect(),
            actions: self
                .actions
                .iter()
                .map(|(s, a)| {
                    let action = match a {
                        Action::Handler(f) => SignalAction::Handler { func: f.clone() },
                        Action::Ignore => SignalAction::Ignore,
                    };
                    (name(*s), action)
                })
                .collect(),
        }
    }

    pub fn in_flight(&self, to: u32) -> impl Iterator<Item = InFlightSignal> + '_ {
        self.pending.iter().map(move |p| InFlightSignal {
            signal: name(p.sig),
            from: p.from.clone(),
            to,
            status: if self.masked(p.sig) {
                SignalStatus::Blocked
            } else {
                SignalStatus::Pending
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TERM: SignalSource = SignalSource::Terminal;

    #[test]
    fn masks_block_and_unblock_but_never_sigkill() {
        let mut s = SigState::default();
        s.set_mask(libc::SIG_BLOCK, bit(libc::SIGINT) | bit(libc::SIGKILL));
        assert!(s.masked(libc::SIGINT));
        assert!(!s.masked(libc::SIGKILL));
        s.set_mask(libc::SIG_UNBLOCK, bit(libc::SIGINT));
        assert_eq!(s.mask, 0);
    }

    #[test]
    fn only_deliverable_signals_interrupt_a_wait() {
        let mut s = SigState::default();
        s.push(libc::SIGCHLD, TERM);
        assert!(!s.interrupts(None), "SIGCHLD por defecto se ignora");
        s.set_action(libc::SIGCHLD, 0x401000, |_| "manejador".into());
        assert!(s.interrupts(None));
        s.set_mask(libc::SIG_BLOCK, bit(libc::SIGCHLD));
        assert!(!s.interrupts(None), "bloqueada por la máscara");
        assert!(s.interrupts(Some(0)), "sigsuspend con máscara vacía la deja pasar");
    }

    #[test]
    fn exec_resets_handlers_but_keeps_ignored_signals() {
        let mut s = SigState::default();
        s.set_action(libc::SIGUSR1, 0x401000, |_| "h".into());
        s.set_action(libc::SIGINT, 1, |_| unreachable!());
        s.exec();
        assert_eq!(s.actions.len(), 1);
        assert_eq!(s.view().actions["SIGINT"], SignalAction::Ignore);
    }

    #[test]
    fn delivery_consumes_the_pending_entry() {
        let mut s = SigState::default();
        s.push(libc::SIGUSR1, TERM);
        s.push(libc::SIGUSR1, TERM);
        assert!(s.take(libc::SIGUSR1).is_some());
        assert_eq!(s.pending.len(), 1);
        assert!(s.take(libc::SIGINT).is_none());
        assert!(s.for_child().pending.is_empty());
    }
}
