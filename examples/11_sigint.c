#include <signal.h>
#include <stdio.h>
#include <unistd.h>

// Presiona Ctrl+C durante la reproducción para interrumpir la cuenta.
volatile sig_atomic_t interrumpido = 0;

void al_ctrl_c(int sig) {
    interrumpido = 1;
}

int main(void) {
    signal(SIGINT, al_ctrl_c);
    int vuelta = 0;
    while (!interrumpido && vuelta < 10) {
        vuelta++;
        sleep(1);
    }
    if (interrumpido)
        printf("me interrumpiste en la vuelta %d\n", vuelta);
    else
        printf("llegué a 10 sin interrupciones\n");
    return 0;
}
