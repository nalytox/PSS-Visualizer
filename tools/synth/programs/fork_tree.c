#include <stdio.h>
#include <unistd.h>

int main(void) {
    for (int i = 0; i < 4; i++)
        fork();
    printf("soy %d\n", getpid());
    return 0;
}
