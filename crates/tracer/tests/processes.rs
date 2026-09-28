//! Casos de procesos que no tienen ejemplo propio: huérfanos, hijos que mueren por señal,
//! system(), espera activa, bloqueo mutuo y stdin compartido.

use pss_tracer::limits::Limits;
use pss_tracer::tracer::{Options, run};
use trace_model::{BlockReason, Event, ExitStatus, Outcome, ProcessState, Trace};

fn trace(source: &str, stdin: &str) -> Trace {
    run(&Options {
        source: source.into(),
        stdin: stdin.into(),
        stdin_eof: false,
        limits: Limits::default(),
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
