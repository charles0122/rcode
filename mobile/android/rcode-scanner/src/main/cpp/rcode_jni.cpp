#include <jni.h>
#include <cstddef>
#include <cstdint>
#include <limits>
#include "rcode.h"

namespace {

struct ResultGuard {
    RCodeResult *value = nullptr;
    ~ResultGuard() { rcode_result_free(value); }
};

jobject make_response(JNIEnv *env, jint status, jbyteArray payload, jint corrected,
                      jdoubleArray landmarks, jint width, jint height,
                      jint spokes, jint ecc_id) {
    jclass type = env->FindClass("com/rcode/scanner/NativeResponse");
    if (type == nullptr) return nullptr;
    jmethodID constructor = env->GetMethodID(type, "<init>", "(I[BI[DIIII)V");
    if (constructor == nullptr) {
        env->DeleteLocalRef(type);
        return nullptr;
    }
    jobject response = env->NewObject(type, constructor, status, payload, corrected,
                                      landmarks, width, height, spokes, ecc_id);
    env->DeleteLocalRef(type);
    return response;
}

jobject failure(JNIEnv *env, uint32_t status) {
    jbyteArray payload = env->NewByteArray(0);
    jdoubleArray landmarks = env->NewDoubleArray(0);
    if (payload == nullptr || landmarks == nullptr) return nullptr;
    jobject response = make_response(env, static_cast<jint>(status), payload, 0,
                                     landmarks, 0, 0, 0, 0);
    env->DeleteLocalRef(payload);
    env->DeleteLocalRef(landmarks);
    return response;
}

}  // namespace

extern "C" JNIEXPORT jobject JNICALL
Java_com_rcode_scanner_NativeBridge_nativeDecodeLuma8(
    JNIEnv *env, jobject /* receiver */, jobject buffer, jint width,
    jint height, jint row_stride) {
    if (buffer == nullptr || width <= 0 || height <= 0 || row_stride <= 0) {
        return failure(env, RCODE_INVALID_ARGUMENT);
    }
    auto *pixels = static_cast<const uint8_t *>(env->GetDirectBufferAddress(buffer));
    const jlong capacity = env->GetDirectBufferCapacity(buffer);
    if (pixels == nullptr || capacity < 0 ||
        static_cast<uint64_t>(capacity) > std::numeric_limits<size_t>::max()) {
        return failure(env, RCODE_INVALID_ARGUMENT);
    }

    ResultGuard result;
    const uint32_t status = rcode_decode_luma8(
        pixels, static_cast<size_t>(capacity), static_cast<uint32_t>(width),
        static_cast<uint32_t>(height), static_cast<size_t>(row_stride), &result.value);
    if (status != RCODE_OK) return failure(env, status);

    const uint8_t *payload_data = nullptr;
    size_t payload_length = 0;
    size_t corrected = 0;
    uint32_t image_width = 0, image_height = 0, spokes = 0, ecc_id = 0;
    if (rcode_result_payload(result.value, &payload_data, &payload_length) != RCODE_OK ||
        payload_length > static_cast<size_t>(std::numeric_limits<jsize>::max()) ||
        rcode_result_corrected_bytes(result.value, &corrected) != RCODE_OK ||
        corrected > static_cast<size_t>(std::numeric_limits<jint>::max()) ||
        rcode_result_image_size(result.value, &image_width, &image_height) != RCODE_OK ||
        rcode_result_profile(result.value, &spokes, &ecc_id) != RCODE_OK) {
        return failure(env, RCODE_INTERNAL_PANIC);
    }

    jbyteArray payload = env->NewByteArray(static_cast<jsize>(payload_length));
    jdoubleArray landmarks = env->NewDoubleArray(8);
    if (payload == nullptr || landmarks == nullptr) return nullptr;
    env->SetByteArrayRegion(payload, 0, static_cast<jsize>(payload_length),
                            reinterpret_cast<const jbyte *>(payload_data));
    jdouble values[8];
    for (uint32_t index = 0; index < 4; ++index) {
        if (rcode_result_landmark(result.value, index, &values[index * 2],
                                  &values[index * 2 + 1]) != RCODE_OK) {
            env->DeleteLocalRef(payload);
            env->DeleteLocalRef(landmarks);
            return failure(env, RCODE_INTERNAL_PANIC);
        }
    }
    env->SetDoubleArrayRegion(landmarks, 0, 8, values);
    if (env->ExceptionCheck()) {
        env->DeleteLocalRef(payload);
        env->DeleteLocalRef(landmarks);
        return nullptr;
    }
    jobject response = make_response(
        env, RCODE_OK, payload, static_cast<jint>(corrected), landmarks,
        static_cast<jint>(image_width), static_cast<jint>(image_height),
        static_cast<jint>(spokes), static_cast<jint>(ecc_id));
    env->DeleteLocalRef(payload);
    env->DeleteLocalRef(landmarks);
    return response;
}
