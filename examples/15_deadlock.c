#include <pthread.h>
#include <stdio.h>
#include <unistd.h>

pthread_mutex_t a = PTHREAD_MUTEX_INITIALIZER;
pthread_mutex_t b = PTHREAD_MUTEX_INITIALIZER;

void *uno(void *arg) {
    pthread_mutex_lock(&a);
    usleep(1000); // trabaja un poco con a tomado
    pthread_mutex_lock(&b);
    printf("uno tiene los dos\n");
    pthread_mutex_unlock(&b);
    pthread_mutex_unlock(&a);
    return NULL;
}

void *dos(void *arg) {
    pthread_mutex_lock(&b);
    usleep(1000);
    pthread_mutex_lock(&a);
    printf("dos tiene los dos\n");
    pthread_mutex_unlock(&a);
    pthread_mutex_unlock(&b);
    return NULL;
}

int main(void) {
    pthread_t t1, t2;
    pthread_create(&t1, NULL, uno, NULL);
    pthread_create(&t2, NULL, dos, NULL);
    pthread_join(t1, NULL);
    pthread_join(t2, NULL);
    return 0;
}
