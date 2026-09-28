#include <stdio.h>
#include <stdlib.h>

struct nodo {
    int valor;
    struct nodo *sig;
};

struct nodo *agregar(struct nodo *cabeza, int valor) {
    struct nodo *n = malloc(sizeof *n);
    n->valor = valor;
    n->sig = cabeza;
    return n;
}

int main(void) {
    struct nodo *lista = NULL;
    for (int i = 1; i <= 4; i++)
        lista = agregar(lista, i * 10);

    int suma = 0;
    for (struct nodo *p = lista; p != NULL; p = p->sig)
        suma += p->valor;
    printf("suma = %d\n", suma);

    struct nodo *segundo = lista->sig;
    free(lista);
    lista = segundo;
    printf("ahora la lista empieza en %d\n", lista->valor);
    return 0;
}
