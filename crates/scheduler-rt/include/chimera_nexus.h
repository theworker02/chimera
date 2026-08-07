#ifndef CHIMERA_NEXUS_H
#define CHIMERA_NEXUS_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct NexusNode NexusNode;

uint32_t chimera_nexus_version(void);

/* budget_ns == 0 → default ~16.6ms (60 FPS) */
NexusNode *chimera_nexus_init(uint64_t budget_ns);
void chimera_nexus_shutdown(NexusNode *node);

/* Returns outcome count; never stalls the engine frame beyond budget accounting. */
int chimera_nexus_tick(NexusNode *node);

int chimera_nexus_submit_rt(
    NexusNode *node,
    uint64_t task_id,
    uint64_t cost_hint_ns,
    uint32_t deadline_ms,
    const uint8_t *local_result_ptr,
    size_t local_result_len
);

uint64_t chimera_nexus_spawn_entity(NexusNode *node);
int chimera_nexus_set_transform(NexusNode *node, uint64_t entity, float x, float y, float z);
int chimera_nexus_get_transform(NexusNode *node, uint64_t entity, float *out_x, float *out_y, float *out_z);

int chimera_nexus_last_error(char *buf, size_t len);

#ifdef __cplusplus
}
#endif

#endif /* CHIMERA_NEXUS_H */
