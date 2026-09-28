#include <signal.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>

void al_terminar_un_hijo(int sig) {
    int status;
    pid_t hijo;
    while ((hijo = waitpid(-1, &status, WNOHANG)) > 0)
        printf("padre: recogí a %d, salió con %d\n", hijo, WEXITSTATUS(status));
}

int main(void) {
    signal(SIGCHLD, al_terminar_un_hijo);
    if (fork() == 0)
        return 7;
    printf("padre: sigo trabajando\n");
    sleep(2);
    printf("padre: termino\n");
    return 0;
}
