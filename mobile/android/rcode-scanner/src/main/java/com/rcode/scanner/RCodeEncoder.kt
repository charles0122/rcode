package com.rcode.scanner

/** Encodes data into platform-neutral drawing commands. */
class RCodeEncoder {
    fun encode(
        payload: ByteArray,
        profile: Profile = Profile(72, EccLevel.M),
        style: RCodeStyle = RCodeStyle.LINES,
    ): RenderPlan {
        val response = NativeBridge.nativeEncodePlan(
            payload,
            profile.spokes,
            profile.ecc.id,
            style.id,
        )
        mapNativeStatus(response.status)?.let { throw ScanException(it) }
        return mapNativePlan(response) ?: throw ScanException(ScanError.INTERNAL)
    }
}
