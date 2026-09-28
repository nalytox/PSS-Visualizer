#include <stdio.h>

struct punto {
    int x;
    int y;
};

struct rect {
    struct punto esquina;
    int ancho;
    int alto;
    char nombre[12];
};

void mover(struct punto *p, int dx, int dy) {
    p->x += dx;
    p->y += dy;
}

int main(void) {
    struct rect r = {{1, 2}, 10, 5, "caja"};
    struct punto pts[3] = {{0, 0}, {3, 4}, {6, 8}};
    struct punto *pp = &pts[1];
    struct rect *pr = &r;
    mover(pp, 1, 1);
    mover(&pr->esquina, -1, 0);
    printf("pts[1] = (%d, %d)\n", pts[1].x, pts[1].y);
    printf("esquina = (%d, %d)\n", r.esquina.x, r.esquina.y);
    return 0;
}
