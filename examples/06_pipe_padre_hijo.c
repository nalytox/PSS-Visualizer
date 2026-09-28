#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    int fd[2];
    pipe(fd);
    if (fork() == 0) {
        close(fd[0]);
        char *msg = "hola papá";
        write(fd[1], msg, strlen(msg));
        close(fd[1]);
        return 0;
    }
    close(fd[1]);
    char buf[32];
    int n;
    while ((n = read(fd[0], buf, sizeof buf - 1)) > 0) {
        buf[n] = '\0';
        printf("el padre leyó: %s\n", buf);
    }
    close(fd[0]);
    wait(NULL);
    return 0;
}
