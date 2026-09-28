#include <signal.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>

volatile sig_atomic_t llego = 0;

void manejador(int sig) {
    llego = 1;
    printf("hijo: recibí la señal %d\n", sig);
}

int main(void) {
    struct sigaction sa = {0};
    sa.sa_handler = manejador;
    sigaction(SIGUSR1, &sa, NULL);
    pid_t pid = fork();
    if (pid == 0) {
        while (!llego)
            pause();
        printf("hijo: sigo y termino\n");
        return 0;
    }
    sleep(1);
    printf("padre: envío SIGUSR1 a %d\n", pid);
    kill(pid, SIGUSR1);
    wait(NULL);
    return 0;
}
