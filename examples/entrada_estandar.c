#include <stdio.h>

int main(void) {
    int n, suma = 0, cuantos = 0;
    printf("Escribe números (EOF para terminar):\n");
    while (scanf("%d", &n) == 1) {
        suma += n;
        cuantos++;
    }
    printf("Leí %d números; la suma es %d\n", cuantos, suma);
    return 0;
}
