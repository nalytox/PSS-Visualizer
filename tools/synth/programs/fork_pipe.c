#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    int fd[2];
    pipe(fd);
    pid_t pid = fork();
    if (pid == 0) {
        close(fd[0]);
        char msg[] = "hola\n";
        write(fd[1], msg, strlen(msg));
        close(fd[1]);
        return 0;
    }
    close(fd[1]);
    char buf[16] = {0};
    int n;
    while ((n = read(fd[0], buf, sizeof buf - 1)) > 0) {
        buf[n] = '\0';
        printf("padre leyó: %s", buf);
    }
    close(fd[0]);
    int status;
    wait(&status);
    printf("hijo terminó con %d\n", WEXITSTATUS(status));
    return 0;
}
