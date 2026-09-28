#include <pthread.h>
#include <stdio.h>

#define N 2
#define TOTAL 4

int buffer[N];
int cantidad = 0, entra = 0, sale = 0;
pthread_mutex_t m = PTHREAD_MUTEX_INITIALIZER;
pthread_cond_t hay_espacio = PTHREAD_COND_INITIALIZER;
pthread_cond_t hay_datos = PTHREAD_COND_INITIALIZER;

void *productor(void *arg) {
    for (int i = 1; i <= TOTAL; i++) {
        pthread_mutex_lock(&m);
        while (cantidad == N)
            pthread_cond_wait(&hay_espacio, &m);
        buffer[entra] = i;
        entra = (entra + 1) % N;
        cantidad++;
        pthread_cond_signal(&hay_datos);
        pthread_mutex_unlock(&m);
    }
    return NULL;
}

void *consumidor(void *arg) {
    for (int i = 0; i < TOTAL; i++) {
        pthread_mutex_lock(&m);
        while (cantidad == 0)
            pthread_cond_wait(&hay_datos, &m);
        int x = buffer[sale];
        sale = (sale + 1) % N;
        cantidad--;
        pthread_cond_signal(&hay_espacio);
        pthread_mutex_unlock(&m);
        printf("consumí %d\n", x);
    }
    return NULL;
}

int main(void) {
    pthread_t p, c;
    pthread_create(&p, NULL, productor, NULL);
    pthread_create(&c, NULL, consumidor, NULL);
    pthread_join(p, NULL);
    pthread_join(c, NULL);
    return 0;
}
