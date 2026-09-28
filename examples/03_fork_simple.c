#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    int x = 10;
    pid_t pid = fork();
    if (pid == 0) {
        x += 5;
        printf("hijo: x = %d\n", x);
        return 3;
    }
    x -= 5;
    printf("padre: x = %d, mi hijo es %d\n", x, pid);
    int status;
    wait(&status);
    printf("el hijo terminó con %d\n", WEXITSTATUS(status));
    return 0;
}
