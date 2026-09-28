#include <stdio.h>
#include <unistd.h>

int main(void) {
    for (int i = 0; i < 3; i++) {
        fork();
    }
    printf("soy %d, hijo de %d\n", getpid(), getppid());
    return 0;
}
