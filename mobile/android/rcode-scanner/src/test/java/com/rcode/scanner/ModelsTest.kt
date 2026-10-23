package com.rcode.scanner

import java.nio.ByteBuffer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ModelsTest {
    @Test
    fun singleImageDecoderRejectsInvalidInputBeforeNativeCall() {
        val decoder = RCodeDecoder()
        assertEquals(
            ScanError.INVALID_IMAGE,
            runCatching { decoder.decodeImage(byteArrayOf()) }
                .exceptionOrNull()
                .let { it as ScanException }
                .error,
        )
        assertEquals(
            ScanError.INVALID_IMAGE,
            runCatching { decoder.decodeLuma(ByteBuffer.allocate(4), 2, 2, 2) }
                .exceptionOrNull()
                .let { it as ScanException }
                .error,
        )
    }

    @Test
    fun nativeResponseMapsToPublicResult() {
        val result = mapNativeResponse(
            NativeResponse(
                status = 0,
                payload = byteArrayOf(1, 2, 3),
                correctedBytes = 2,
                landmarks = doubleArrayOf(10.0, 20.0, 90.0, 20.0, 10.0, 90.0, 90.0, 90.0),
                width = 100,
                height = 100,
                spokes = 54,
                eccId = 2,
            ),
        )

        assertNotNull(result)
        assertEquals(listOf<Byte>(1, 2, 3), result!!.payload.toList())
        assertEquals(2, result.correctedBytes)
        assertEquals(Profile(54, EccLevel.Q), result.profile)
        assertEquals(100, result.imageWidth)
        assertEquals(100, result.imageHeight)
        assertEquals(4, result.landmarks.size)
        assertEquals(Landmark(10f, 20f), result.landmarks.first())
    }

    @Test
    fun statusMappingAndOneShotDeduplicationAreStable() {
        assertEquals(ScanError.NO_CODE, mapNativeStatus(2))
        assertEquals(ScanError.INVALID_IMAGE, mapNativeStatus(4))
        assertEquals(ScanError.INTERNAL, mapNativeStatus(99))

        val gate = OneShotGate()
        assertTrue(gate.claim())
        assertFalse(gate.claim())
        gate.reset()
        assertTrue(gate.claim())
    }

    @Test
    fun previewCoordinatesApplyCameraXTransform() {
        val mapped = PreviewCoordinates.map(
            points = listOf(Landmark(12f, 23f)),
            matrix = floatArrayOf(0f, -1f, 100f, 1f, 0f, 0f, 0f, 0f, 1f),
        )

        assertEquals(listOf(Landmark(77f, 12f)), mapped)
        assertNull(PreviewCoordinates.map(emptyList(), floatArrayOf(1f)))
    }
}
