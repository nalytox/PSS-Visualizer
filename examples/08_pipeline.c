#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>

// Equivale a: ls | grep c | wc -l
int main(void) {
    int a[2], b[2];
    pipe(a);
    pipe(b);
    if (fork() == 0) {
        dup2(a[1], 1);
        close(a[0]); close(a[1]); close(b[0]); close(b[1]);
        execlp("ls", "ls", NULL);
        return 1;
    }
    if (fork() == 0) {
        dup2(a[0], 0);
        dup2(b[1], 1);
        close(a[0]); close(a[1]); close(b[0]); close(b[1]);
        execlp("grep", "grep", "c", NULL);
        return 1;
    }
    if (fork() == 0) {
        dup2(b[0], 0);
        close(a[0]); close(a[1]); close(b[0]); close(b[1]);
        execlp("wc", "wc", "-l", NULL);
        return 1;
    }
    close(a[0]); close(a[1]); close(b[0]); close(b[1]);
    while (wait(NULL) > 0)
        ;
    return 0;
}
