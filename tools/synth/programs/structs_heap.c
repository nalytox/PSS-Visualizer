#include <stdio.h>
#include <stdlib.h>

struct point {
    int x;
    int y;
};

struct rect {
    char name[8];
    struct point corner;
    struct point *center;
};

struct node {
    int value;
    struct node *next;
};

struct node *push(struct node *head, int value) {
    struct node *n = malloc(sizeof *n);
    n->value = value;
    n->next = head;
    return n;
}

int main(void) {
    struct point pts[2] = {{1, 2}, {3, 4}};
    struct rect r = {"caja", {5, 6}, &pts[1]};
    struct node *list = NULL;
    for (int i = 1; i <= 3; i++)
        list = push(list, i * 10);
    struct node *second = list->next;
    struct node *old = list;
    free(list);
    list = NULL;
    printf("second->value = %d\n", second->value);
    return 0;
}
