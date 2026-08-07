/* Minimal C embedding example — syntax-validated; compile when a C toolchain is available:
 *
 *   cargo build -p chimera-nexus --release
 *   # link against target/.../chimera_nexus.dll / .lib
 *   cl /I..\include example.c /link chimera_nexus.lib
 *
 * Untested against Godot/Unreal on this host.
 */
#include <stdio.h>
#include "../include/chimera_nexus.h"

int main(void) {
    printf("nexus version=0x%08x\n", chimera_nexus_version());
    NexusNode *n = chimera_nexus_init(0);
    if (!n) return 1;
    uint64_t e = chimera_nexus_spawn_entity(n);
    chimera_nexus_set_transform(n, e, 1.0f, 2.0f, 3.0f);
    unsigned char fallback[] = { 1, 2, 3 };
    chimera_nexus_submit_rt(n, 42, 1000, 8, fallback, sizeof(fallback));
    int outcomes = chimera_nexus_tick(n);
    float x, y, z;
    if (chimera_nexus_get_transform(n, e, &x, &y, &z) == 0) {
        printf("entity=%llu transform=(%.1f,%.1f,%.1f) outcomes=%d\n",
               (unsigned long long)e, x, y, z, outcomes);
    }
    chimera_nexus_shutdown(n);
    return 0;
}
