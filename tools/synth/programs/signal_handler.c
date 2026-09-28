#include <signal.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>

volatile sig_atomic_t got = 0;

void on_usr1(int sig) {
    got = 1;
    printf("hijo: recibí la señal %d\n", sig);
}

int main(void) {
    struct sigaction sa = {0};
    sa.sa_handler = on_usr1;
    sigaction(SIGUSR1, &sa, NULL);
    pid_t pid = fork();
    if (pid == 0) {
        while (!got)
            pause();
        printf("hijo: termino\n");
        return 0;
    }
    sleep(1);
    kill(pid, SIGUSR1);
    wait(NULL);
    printf("padre: listo\n");
    return 0;
}
