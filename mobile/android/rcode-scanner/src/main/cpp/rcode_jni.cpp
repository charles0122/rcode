#include <jni.h>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <vector>
#include "rcode.h"

namespace {

struct ResultGuard {
    RCodeResult *value = nullptr;
    ~ResultGuard() { rcode_result_free(value); }
};

struct PlanGuard {
    RCodePlan *value = nullptr;
    ~PlanGuard() { rcode_plan_free(value); }
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

jobject make_plan_response(JNIEnv *env, jint status, jint version, jdouble viewport,
                           jdouble logo_x, jdouble logo_y, jdouble logo_radius,
                           jintArray kinds, jintArray paints, jdoubleArray values) {
    jclass type = env->FindClass("com/rcode/scanner/NativePlanResponse");
    if (type == nullptr) return nullptr;
    jmethodID constructor = env->GetMethodID(type, "<init>", "(IIDDDD[I[I[D)V");
    if (constructor == nullptr) {
        env->DeleteLocalRef(type);
        return nullptr;
    }
    jobject response = env->NewObject(type, constructor, status, version, viewport, logo_x,
                                      logo_y, logo_radius, kinds, paints, values);
    env->DeleteLocalRef(type);
    return response;
}

jobject plan_failure(JNIEnv *env, uint32_t status) {
    jintArray kinds = env->NewIntArray(0);
    jintArray paints = env->NewIntArray(0);
    jdoubleArray values = env->NewDoubleArray(0);
    if (kinds == nullptr || paints == nullptr || values == nullptr) return nullptr;
    jobject response = make_plan_response(env, static_cast<jint>(status), 0, 0, 0, 0, 0,
                                          kinds, paints, values);
    env->DeleteLocalRef(kinds);
    env->DeleteLocalRef(paints);
    env->DeleteLocalRef(values);
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

extern "C" JNIEXPORT jobject JNICALL
Java_com_rcode_scanner_NativeBridge_nativeEncodePlan(
    JNIEnv *env, jobject /* receiver */, jbyteArray payload, jint spokes,
    jint ecc_id, jint style_id) {
    if (payload == nullptr || spokes <= 0 || ecc_id < 0 || style_id < 0) {
        return plan_failure(env, RCODE_INVALID_ARGUMENT);
    }
    const jsize payload_length = env->GetArrayLength(payload);
    std::vector<uint8_t> bytes(static_cast<size_t>(payload_length));
    if (payload_length > 0) {
        env->GetByteArrayRegion(payload, 0, payload_length,
                               reinterpret_cast<jbyte *>(bytes.data()));
        if (env->ExceptionCheck()) return nullptr;
    }

    PlanGuard plan;
    const uint32_t status = rcode_encode_plan(
        bytes.data(), bytes.size(), static_cast<uint32_t>(spokes),
        static_cast<uint32_t>(ecc_id), static_cast<uint32_t>(style_id), &plan.value);
    if (status != RCODE_OK) return plan_failure(env, status);

    uint32_t version = 0;
    double viewport = 0, logo_x = 0, logo_y = 0, logo_radius = 0;
    const RCodeDrawOperation *operations = nullptr;
    size_t count = 0;
    if (rcode_plan_version(plan.value, &version) != RCODE_OK ||
        rcode_plan_metadata(plan.value, &viewport, &logo_x, &logo_y, &logo_radius) != RCODE_OK ||
        rcode_plan_operations(plan.value, &operations, &count) != RCODE_OK ||
        count > static_cast<size_t>(std::numeric_limits<jsize>::max()) ||
        count > static_cast<size_t>(std::numeric_limits<jsize>::max()) / 6) {
        return plan_failure(env, RCODE_INTERNAL_PANIC);
    }

    std::vector<jint> kinds(count), paints(count);
    std::vector<jdouble> values(count * 6);
    for (size_t index = 0; index < count; ++index) {
        kinds[index] = static_cast<jint>(operations[index].kind);
        paints[index] = static_cast<jint>(operations[index].paint);
        for (size_t value = 0; value < 6; ++value) {
            values[index * 6 + value] = operations[index].values[value];
        }
    }

    jintArray kind_array = env->NewIntArray(static_cast<jsize>(count));
    jintArray paint_array = env->NewIntArray(static_cast<jsize>(count));
    jdoubleArray value_array = env->NewDoubleArray(static_cast<jsize>(count * 6));
    if (kind_array == nullptr || paint_array == nullptr || value_array == nullptr) return nullptr;
    env->SetIntArrayRegion(kind_array, 0, static_cast<jsize>(count), kinds.data());
    env->SetIntArrayRegion(paint_array, 0, static_cast<jsize>(count), paints.data());
    env->SetDoubleArrayRegion(value_array, 0, static_cast<jsize>(count * 6), values.data());
    if (env->ExceptionCheck()) return nullptr;
    jobject response = make_plan_response(env, RCODE_OK, static_cast<jint>(version), viewport, logo_x, logo_y,
                                          logo_radius, kind_array, paint_array, value_array);
    env->DeleteLocalRef(kind_array);
    env->DeleteLocalRef(paint_array);
    env->DeleteLocalRef(value_array);
    return response;
}
