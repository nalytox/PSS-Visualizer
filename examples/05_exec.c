#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    printf("antes del exec\n");
    pid_t pid = fork();
    if (pid == 0) {
        execlp("ls", "ls", NULL);
        perror("execlp");
        return 1;
    }
    waitpid(pid, NULL, 0);
    printf("ls terminó\n");
    return 0;
}
