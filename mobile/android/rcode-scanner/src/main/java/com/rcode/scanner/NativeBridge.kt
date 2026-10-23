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
}
