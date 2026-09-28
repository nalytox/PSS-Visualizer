#include <pthread.h>
#include <stdio.h>

int contador = 0;
pthread_mutex_t candado = PTHREAD_MUTEX_INITIALIZER;

void *sumar(void *arg) {
    for (int i = 0; i < 3; i++) {
        pthread_mutex_lock(&candado);
        int leido = contador;
        leido = leido + 1;
        contador = leido;
        pthread_mutex_unlock(&candado);
    }
    return NULL;
}

int main(void) {
    pthread_t a, b;
    pthread_create(&a, NULL, sumar, NULL);
    pthread_create(&b, NULL, sumar, NULL);
    pthread_join(a, NULL);
    pthread_join(b, NULL);
    printf("contador = %d\n", contador);
    return 0;
}
