#ifndef RCODE_H
#define RCODE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct RCodeResult RCodeResult;

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
