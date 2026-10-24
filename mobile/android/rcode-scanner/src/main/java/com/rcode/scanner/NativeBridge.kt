package com.rcode.scanner

import java.nio.ByteBuffer

internal class NativeResponse(
    val status: Int,
    val payload: ByteArray,
    val correctedBytes: Int,
    val landmarks: DoubleArray,
    val width: Int,
    val height: Int,
    val spokes: Int,
    val eccId: Int,
)

internal class NativePlanResponse(
    val status: Int,
    val version: Int,
    val viewport: Double,
    val logoX: Double,
    val logoY: Double,
    val logoRadius: Double,
    val kinds: IntArray,
    val paints: IntArray,
    val values: DoubleArray,
)

internal object NativeBridge {
    init {
        System.loadLibrary("rcode_jni")
    }

    external fun nativeDecodeLuma8(
        pixels: ByteBuffer,
        width: Int,
        height: Int,
        rowStride: Int,
    ): NativeResponse

    external fun nativeEncodePlan(
        payload: ByteArray,
        spokes: Int,
        eccId: Int,
        styleId: Int,
    ): NativePlanResponse
}
