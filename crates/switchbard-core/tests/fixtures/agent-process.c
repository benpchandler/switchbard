/* Harmless unit-owned native agent identity fixture. No subprocesses. */
#include <signal.h>
#include <stdio.h>
#include <unistd.h>
int main(int argc, char **argv) {
    alarm(60);
    if (argc > 1) {
        if (signal(SIGTERM, SIG_IGN) == SIG_ERR) return 2;
        FILE *ready = fopen(argv[1], "w");
        if (!ready) return 3;
        if (fclose(ready) != 0) return 4;
    }
    for (;;) pause();
}
