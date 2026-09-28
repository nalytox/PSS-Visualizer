#include <stdio.h>
#include <string.h>
#include <unistd.h>

int main(void) {
    int fd[2];
    pipe(fd);
    if (fork() == 0) {
        close(fd[0]);
        write(fd[1], "dato", 4);
        close(fd[1]);
        return 0;
    }
    // Olvido: el padre no cierra fd[1], así que el pipe nunca queda sin escritores.
    char buf[16];
    int n;
    while ((n = read(fd[0], buf, sizeof buf)) > 0) {
        printf("leí %d bytes\n", n);
    }
    printf("fin de archivo\n");
    return 0;
}
