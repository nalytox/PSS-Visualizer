#include <pthread.h>
#include <stdio.h>

// Dos hilos suman al mismo contador sin mutex. Leer, sumar y guardar son tres pasos: si el
// planificador cambia de hilo entre ellos, se pierde una suma. Prueba el modo manual.
int contador = 0;

void *sumar(void *arg) {
    for (int i = 0; i < 3; i++) {
        int leido = contador;
        leido = leido + 1;
        contador = leido;
    }
    return NULL;
}

int main(void) {
    pthread_t a, b;
    pthread_create(&a, NULL, sumar, NULL);
    pthread_create(&b, NULL, sumar, NULL);
    pthread_join(a, NULL);
    pthread_join(b, NULL);
    printf("contador = %d (debería ser 6)\n", contador);
    return 0;
}
