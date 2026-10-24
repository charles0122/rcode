#ifndef RCODE_H
#define RCODE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct RCodeResult RCodeResult;
typedef struct RCodePlan RCodePlan;

typedef struct RCodeDrawOperation {
    /* circle=0, round line=1, round arc=2 */
    uint32_t kind;
    /* background=0, foreground=1 */
    uint32_t paint;
    double values[6];
} RCodeDrawOperation;

enum RCodeStatus {
    RCODE_OK = 0,
    RCODE_INVALID_ARGUMENT = 1,
    RCODE_NO_CODE = 2,
    RCODE_CORRUPT_CODE = 3,
    RCODE_INVALID_IMAGE = 4,
    RCODE_INTERNAL_PANIC = 5
};

uint32_t rcode_abi_version(void);
const char *rcode_core_version(void);
const char *rcode_status_message(uint32_t status);

/* pixels must address pixels_len readable bytes; row_stride is in bytes. */
uint32_t rcode_decode_luma8(const uint8_t *pixels, size_t pixels_len,
                            uint32_t width, uint32_t height, size_t row_stride,
                            RCodeResult **out_result);

/* style ids are lines=0, dots=1, rings=2. */
uint32_t rcode_encode_plan(const uint8_t *payload, size_t payload_len,
                           uint32_t spokes, uint32_t ecc_id, uint32_t style_id,
                           RCodePlan **out_plan);
uint32_t rcode_plan_version(const RCodePlan *plan, uint32_t *out_version);
uint32_t rcode_plan_metadata(const RCodePlan *plan, double *out_viewport,
                             double *out_logo_x, double *out_logo_y,
                             double *out_logo_radius);
/* Borrowed operations remain valid only until rcode_plan_free(plan). */
uint32_t rcode_plan_operations(const RCodePlan *plan,
                               const RCodeDrawOperation **out_operations,
                               size_t *out_count);
void rcode_plan_free(RCodePlan *plan);

/* ECC ids are L=0, M=1, Q=2, H=3. */
/* Borrowed payload bytes remain valid only until rcode_result_free(result). */
uint32_t rcode_result_payload(const RCodeResult *result,
                              const uint8_t **out_payload, size_t *out_length);
uint32_t rcode_result_corrected_bytes(const RCodeResult *result, size_t *out_count);
uint32_t rcode_result_landmark(const RCodeResult *result, uint32_t index,
                               double *out_x, double *out_y);
uint32_t rcode_result_image_size(const RCodeResult *result,
                                 uint32_t *out_width, uint32_t *out_height);
uint32_t rcode_result_profile(const RCodeResult *result,
                              uint32_t *out_spokes, uint32_t *out_ecc_id);
void rcode_result_free(RCodeResult *result);

#ifdef __cplusplus
}
#endif

#endif
