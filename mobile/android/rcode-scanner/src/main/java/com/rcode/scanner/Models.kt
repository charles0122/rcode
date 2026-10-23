package com.rcode.scanner

data class Landmark(val x: Float, val y: Float)

enum class EccLevel(val id: Int) { L(0), M(1), Q(2), H(3) }

data class Profile(val spokes: Int, val ecc: EccLevel)

data class ScanResult(
    val payload: ByteArray,
    val correctedBytes: Int,
    val landmarks: List<Landmark>,
    val imageWidth: Int,
    val imageHeight: Int,
    val profile: Profile,
)

enum class ScanError {
    PERMISSION_REQUIRED,
    CAMERA_UNAVAILABLE,
    NO_CODE,
    INVALID_ARGUMENT,
    CORRUPT_CODE,
    INVALID_IMAGE,
    INTERNAL,
}

enum class CameraPermissionState { GRANTED, REQUIRED }

internal fun mapNativeStatus(status: Int): ScanError? = when (status) {
    0 -> null
    1 -> ScanError.INVALID_ARGUMENT
    2 -> ScanError.NO_CODE
    3 -> ScanError.CORRUPT_CODE
    4 -> ScanError.INVALID_IMAGE
    else -> ScanError.INTERNAL
}

internal fun mapNativeResponse(response: NativeResponse): ScanResult? {
    if (response.status != 0 || response.landmarks.size != 8) return null
    val ecc = EccLevel.entries.firstOrNull { it.id == response.eccId } ?: return null
    if (response.spokes !in listOf(36, 54, 72)) return null
    val points = response.landmarks.asList().chunked(2).map { (x, y) ->
        if (!x.isFinite() || !y.isFinite()) return null
        Landmark(x.toFloat(), y.toFloat())
    }
    return ScanResult(
        payload = response.payload,
        correctedBytes = response.correctedBytes,
        landmarks = points,
        imageWidth = response.width,
        imageHeight = response.height,
        profile = Profile(response.spokes, ecc),
    )
}

internal class OneShotGate {
    private var delivered = false

    @Synchronized
    fun reset() {
        delivered = false
    }

    @Synchronized
    fun claim(): Boolean {
        if (delivered) return false
        delivered = true
        return true
    }

    @Synchronized
    fun isClaimed(): Boolean = delivered
}

internal object PreviewCoordinates {
    fun map(
        points: List<Landmark>,
        matrix: FloatArray,
    ): List<Landmark>? {
        if (matrix.size != 9) return null
        return points.map { point ->
            val x = point.x
            val y = point.y
            val denominator = matrix[6] * x + matrix[7] * y + matrix[8]
            if (!denominator.isFinite() || kotlin.math.abs(denominator) < 1e-8f) return null
            val mappedX = (matrix[0] * x + matrix[1] * y + matrix[2]) / denominator
            val mappedY = (matrix[3] * x + matrix[4] * y + matrix[5]) / denominator
            if (!mappedX.isFinite() || !mappedY.isFinite()) return null
            Landmark(mappedX, mappedY)
        }
    }
}
