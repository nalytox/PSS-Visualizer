//! Casos de procesos que no tienen ejemplo propio: huérfanos, hijos que mueren por señal,
//! system(), espera activa, bloqueo mutuo y stdin compartido.

use pss_tracer::limits::Limits;
use pss_tracer::tracer::{Options, run};
use trace_model::{BlockReason, Event, ExitStatus, Outcome, ProcessState, Trace};

fn trace(source: &str, stdin: &str) -> Trace {
    trace_with(source, stdin, vec![])
}

fn trace_with(source: &str, stdin: &str, injections: Vec<(u64, i32)>) -> Trace {
    run(&Options {
        source: source.into(),
        stdin: stdin.into(),
        stdin_eof: false,
        limits: Limits::default(),
        injections,
        policy: trace_model::Policy::RoundRobin,
        seed: 0,
        schedule: vec![],
    })
}

fn events(t: &Trace) -> impl Iterator<Item = &Event> {
    t.steps.iter().flat_map(|s| &s.events)
}

fn output(t: &Trace) -> Vec<(u32, &str)> {
    t.output.iter().map(|c| (c.pid, c.bytes.as_str())).collect()
}

#[test]
fn orphans_are_adopted_by_init() {
    let t = trace(
        r#"#include <stdio.h>
#include <unistd.h>
int main(void) {
    if (fork() == 0) {
        sleep(2);
        printf("padre %d\n", getppid());
        return 0;
    }
    return 0;
}
"#,
        "",
    );
    assert!(matches!(t.outcome, Outcome::Exited { code: 0 }), "{:?}", t.outcome);
    assert_eq!(output(&t), [(1001, "padre 1\n")]);
    assert!(events(&t).any(|e| matches!(
        e,
        Event::Reparent {
            pid: 1001,
            from: 1000,
            to: 1
        }
    )));
    // El reloj virtual saltó los 2 s de sleep: nadie más podía avanzar.
    assert!(t.steps.last().unwrap().clock >= 2000);
    let child = t
        .steps
        .last()
        .unwrap()
        .processes
        .iter()
        .find(|p| p.pid == 1001)
        .unwrap();
    assert_eq!(child.state, ProcessState::Reaped);
    assert_eq!(child.ppid, Some(1));
}

#[test]
fn children_killed_by_signals_are_reported_to_wait() {
    let t = trace(
        r#"#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
int main(void) {
    if (fork() == 0) { int *p = 0; *p = 1; }
    if (fork() == 0) { abort(); }
    int st;
    for (int k = 0; k < 2; k++) {
        pid_t w = wait(&st);
        printf("%d %d\n", w, WTERMSIG(st));
    }
    return 0;
}
"#,
        "",
    );
    assert!(matches!(t.outcome, Outcome::Exited { code: 0 }), "{:?}", t.outcome);
    assert_eq!(output(&t), [(1000, "1001 11\n"), (1000, "1002 6\n")]);
    let reaped: Vec<_> = events(&t)
        .filter_map(|e| match e {
            Event::Wait {
                reaped: Some(r),
                status: Some(ExitStatus::Signal { signal, .. }),
                ..
            } => Some((*r, signal.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(reaped, [(1001, "SIGSEGV"), (1002, "SIGABRT")]);
    assert_eq!(t.summary.mem_errors.len(), 1);
    assert_eq!(t.summary.mem_errors[0].pid, 1001);
}

#[test]
fn system_runs_a_shell_as_a_black_box() {
    let t = trace(
        r#"#include <stdio.h>
#include <stdlib.h>
int main(void) {
    int r = system("echo hola");
    printf("r = %d\n", r);
    return 0;
}
"#,
        "",
    );
    assert!(matches!(t.outcome, Outcome::Exited { code: 0 }), "{:?}", t.outcome);
    assert_eq!(output(&t), [(1001, "hola\n"), (1000, "r = 0\n")]);
    assert!(events(&t).any(|e| matches!(e, Event::Fork { vfork: true, .. })));
    assert!(events(&t).any(|e| matches!(
        e,
        Event::Exec {
            pid: 1001,
            blackbox: true,
            ..
        }
    )));
}

#[test]
fn busy_waiting_lets_a_sleeping_child_wake_up() {
    let t = trace(
        r#"#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>
int main(void) {
    pid_t p = fork();
    if (p == 0) { usleep(5000); return 7; }
    int n = 0, st;
    while (waitpid(p, &st, WNOHANG) == 0) n++;
    printf("%d\n", WEXITSTATUS(st));
    return 0;
}
"#,
        "",
    );
    assert!(matches!(t.outcome, Outcome::Exited { code: 0 }), "{:?}", t.outcome);
    assert_eq!(output(&t), [(1000, "7\n")]);
}

#[test]
fn everyone_blocked_is_a_deadlock() {
    let t = trace(
        r#"#include <sys/wait.h>
#include <unistd.h>
int main(void) {
    if (fork() == 0) { pause(); }
    wait(NULL);
    return 0;
}
"#,
        "",
    );
    let Outcome::Deadlock { tasks } = &t.outcome else {
        panic!("{:?}", t.outcome);
    };
    assert_eq!(tasks.len(), 2);
    let last = t.steps.last().unwrap();
    let blocked: Vec<_> = last.processes.iter().map(|p| p.threads[0].blocked_on.clone()).collect();
    assert_eq!(
        blocked,
        [Some(BlockReason::Wait { target: -1 }), Some(BlockReason::Pause)]
    );
}

#[test]
fn stdin_is_shared_and_the_second_reader_waits_for_more() {
    let t = trace(
        r#"#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>
int main(void) {
    int x;
    if (fork() == 0) { scanf("%d", &x); printf("hijo %d\n", x); return 0; }
    wait(NULL);
    scanf("%d", &x);
    return 0;
}
"#,
        "5\n",
    );
    assert!(
        matches!(t.outcome, Outcome::AwaitingInput { pid: 1000, .. }),
        "{:?}",
        t.outcome
    );
    assert_eq!(output(&t), [(1001, "hijo 5\n")]);
}

#[test]
fn process_ids_are_virtual_and_stable() {
    let src = r#"#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>
int main(void) {
    pid_t c = fork();
    if (c == 0) { printf("%d %d %d\n", getpid(), getppid(), getpgrp()); return 0; }
    pid_t w = waitpid(c, NULL, 0);
    printf("%d %d\n", c, w);
    return 0;
}
"#;
    let t = trace(src, "");
    assert_eq!(output(&t), [(1001, "1001 1000 1000\n"), (1000, "1001 1001\n")]);
    assert_eq!(trace(src, "").steps, t.steps);
}

#[test]
fn writing_to_a_pipe_without_readers_raises_sigpipe() {
    let t = trace(
        r#"#include <unistd.h>
int main(void) {
    int fd[2];
    pipe(fd);
    close(fd[0]);
    write(fd[1], "x", 1);
    return 0;
}
"#,
        "",
    );
    assert!(
        matches!(&t.outcome, Outcome::Signaled { signal } if signal == "SIGPIPE"),
        "{:?}",
        t.outcome
    );
    assert!(events(&t).any(|e| matches!(e, Event::Write { epipe: true, .. })));
    assert!(events(&t).any(|e| matches!(e, Event::SignalDeliver { signal, .. } if signal == "SIGPIPE")));
}

#[test]
fn a_full_pipe_blocks_the_writer_until_someone_reads() {
    let t = trace(
        r#"#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
static char big[70000];
int main(void) {
    int fd[2];
    pipe(fd);
    if (fork() == 0) {
        close(fd[0]);
        write(fd[1], big, 65536);
        write(fd[1], big, 100);
        return 0;
    }
    close(fd[1]);
    long total = 0, n;
    while ((n = read(fd[0], big, sizeof big)) > 0) total += n;
    wait(NULL);
    printf("%ld\n", total);
    return 0;
}
"#,
        "",
    );
    assert!(matches!(t.outcome, Outcome::Exited { code: 0 }), "{:?}", t.outcome);
    assert_eq!(output(&t), [(1000, "65636\n")]);
}

#[test]
fn a_masked_signal_waits_until_it_is_unblocked() {
    let t = trace(
        r#"#include <signal.h>
#include <stdio.h>
#include <unistd.h>
void h(int s) { printf("llego\n"); }
int main(void) {
    signal(SIGUSR1, h);
    sigset_t m;
    sigemptyset(&m);
    sigaddset(&m, SIGUSR1);
    sigprocmask(SIG_BLOCK, &m, NULL);
    raise(SIGUSR1);
    printf("bloqueada\n");
    sigprocmask(SIG_UNBLOCK, &m, NULL);
    printf("fin\n");
    return 0;
}
"#,
        "",
    );
    assert_eq!(output(&t), [(1000, "bloqueada\n"), (1000, "llego\n"), (1000, "fin\n")]);
    let blocked = t
        .steps
        .iter()
        .any(|s| s.signals.iter().any(|f| f.status == trace_model::SignalStatus::Blocked));
    assert!(blocked, "la señal pendiente se ve bloqueada por la máscara");
}

#[test]
fn alarm_fires_on_the_virtual_clock() {
    let t = trace(
        r#"#include <signal.h>
#include <stdio.h>
#include <unistd.h>
void h(int s) { printf("alarma\n"); }
int main(void) {
    signal(SIGALRM, h);
    alarm(3);
    pause();
    printf("despues\n");
    return 0;
}
"#,
        "",
    );
    assert_eq!(output(&t), [(1000, "alarma\n"), (1000, "despues\n")]);
    assert!(events(&t).any(|e| matches!(
        e,
        Event::SignalSend {
            from: trace_model::SignalSource::Timer { pid: 1000 },
            ..
        }
    )));
    assert!(t.steps.last().unwrap().clock >= 3000);
}

#[test]
fn sigkill_ends_a_blocked_child_at_once() {
    let t = trace(
        r#"#include <signal.h>
#include <sys/wait.h>
#include <unistd.h>
int main(void) {
    pid_t p = fork();
    if (p == 0) { pause(); return 0; }
    kill(p, SIGKILL);
    int st;
    wait(&st);
    return WTERMSIG(st);
}
"#,
        "",
    );
    assert!(matches!(t.outcome, Outcome::Exited { code: 9 }), "{:?}", t.outcome);
}

#[test]
fn threads_receive_their_argument_and_return_a_value() {
    let t = trace(
        r#"#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
void *doble(void *arg) {
    intptr_t n = (intptr_t)arg;
    return (void *)(n * 2);
}
int main(void) {
    pthread_t h;
    void *r;
    pthread_create(&h, NULL, doble, (void *)21);
    pthread_join(h, &r);
    printf("%ld\n", (long)(intptr_t)r);
    return 0;
}
"#,
        "",
    );
    assert_eq!(output(&t), [(1000, "42\n")]);
    let join = events(&t).find_map(|e| match e {
        Event::Join { target, retval, .. } => Some((*target, retval.clone())),
        _ => None,
    });
    let (target, retval) = join.unwrap();
    assert_eq!(target, 1001);
    assert!(matches!(retval, Some(trace_model::Value::Scalar { repr: Some(r), .. }) if r == "0x2a"));
}

#[test]
fn pthread_exit_in_main_lets_the_other_threads_finish() {
    let t = trace(
        r#"#include <pthread.h>
#include <stdio.h>
void *hola(void *arg) { printf("hilo\n"); return NULL; }
int main(void) {
    pthread_t h;
    pthread_create(&h, NULL, hola, NULL);
    pthread_exit(NULL);
}
"#,
        "",
    );
    assert!(matches!(t.outcome, Outcome::Exited { code: 0 }), "{:?}", t.outcome);
    assert_eq!(output(&t), [(1000, "hilo\n")]);
}

#[test]
fn setitimer_real_uses_the_virtual_clock_too() {
    let t = trace(
        r#"#include <signal.h>
#include <stdio.h>
#include <sys/time.h>
#include <unistd.h>
void h(int s) { printf("tic\n"); }
int main(void) {
    signal(SIGALRM, h);
    struct itimerval v = {{0, 0}, {1, 500000}};
    setitimer(ITIMER_REAL, &v, NULL);
    pause();
    return 0;
}
"#,
        "",
    );
    assert_eq!(output(&t), [(1000, "tic\n")]);
    assert!(t.steps.last().unwrap().clock >= 1500);
}

#[test]
fn frame_offsets_come_from_the_unwind_information() {
    let dir = std::env::temp_dir().join(format!("pss-cfi-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = "int f(int a) { int b[40]; b[0] = a; return b[0]; }\nint main(void) { return f(1); }\n";
    let compiled = pss_tracer::compile::compile(&dir, src).unwrap();
    let debug = pss_tracer::dwarf::DebugInfo::load(&compiled.binary, "prog.c").unwrap();
    for f in &debug.functions {
        // Con frame pointer en x86_64 el CFA está siempre 16 bytes sobre rbp; en aarch64 depende del
        // tamaño del frame, así que solo se exige que exista.
        if cfg!(target_arch = "x86_64") {
            assert_eq!(f.cfa_fp, 16, "{}", f.name);
        } else {
            assert!(f.cfa_fp >= 16, "{}", f.name);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ppoll_without_descriptors_waits_like_pause_or_sleep() {
    let t = trace(
        r#"#define _GNU_SOURCE
#include <poll.h>
#include <stdio.h>
#include <time.h>
int main(void) {
    struct timespec ts = {2, 0};
    ppoll(NULL, 0, &ts, NULL);
    printf("listo\n");
    ppoll(NULL, 0, NULL, NULL);
    return 0;
}
"#,
        "",
    );
    assert_eq!(output(&t), [(1000, "listo\n")]);
    assert!(matches!(t.outcome, Outcome::Deadlock { .. }), "{:?}", t.outcome);
    assert!(t.steps.last().unwrap().clock >= 2000);
}

#[test]
fn a_binary_that_cannot_be_executed_reports_why() {
    use std::os::unix::fs::PermissionsExt;
    // Como en un /tmp montado con noexec: el exec falla con EACCES y se informa, en vez de un
    // "no pudo iniciarse" sin causa.
    let dir = std::env::temp_dir().join(format!("pss-noexec-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("prog");
    std::fs::write(&bin, b"\x7fELF").unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o644)).unwrap();
    let r = pss_tracer::launch::launch(&dir, "prog", b"", true, &Limits::default());
    assert_eq!(r.err(), Some(nix::errno::Errno::EACCES));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn several_threads_waiting_on_a_mutex_all_get_their_turn() {
    // Al liberar el mutex despiertan los dos que esperaban; uno lo toma primero y el otro debe
    // volver a esperar en el modelo, no dormirse en el kernel (antes la traza se cortaba a los 10 s).
    let t = trace(
        r#"#include <stdio.h>
#include <unistd.h>
#include <pthread.h>
long saldo = 1000;
int aprobados = 0;
pthread_mutex_t mutex = PTHREAD_MUTEX_INITIALIZER;
void *retirar(void *arg) {
    pthread_mutex_lock(&mutex);
    if (saldo >= 1000) {
        usleep(1000);
        saldo -= 1000;
        aprobados++;
    }
    pthread_mutex_unlock(&mutex);
    return NULL;
}
int main(void) {
    pthread_t h[4];
    for (int k = 0; k < 4; k++)
        pthread_create(&h[k], NULL, retirar, NULL);
    for (int k = 0; k < 4; k++)
        pthread_join(h[k], NULL);
    printf("%d %ld\n", aprobados, saldo);
    return 0;
}
"#,
        "",
    );
    assert!(matches!(t.outcome, Outcome::Exited { code: 0 }), "{:?}", t.outcome);
    assert_eq!(output(&t), [(1000, "1 0\n")]);
}
