package com.rcode.scanner

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.media.ExifInterface
import java.io.ByteArrayInputStream
import java.nio.ByteBuffer

class ScanException(val error: ScanError) : Exception(error.name)

/** Synchronous single-frame decoder. Camera capture and threading remain caller-owned. */
class RCodeDecoder {
    fun decodeLuma(
        pixels: ByteBuffer,
        width: Int,
        height: Int,
        rowStride: Int,
    ): ScanResult {
        if (!pixels.isDirect || width < 2 || height < 2 || rowStride < width) {
            throw ScanException(ScanError.INVALID_IMAGE)
        }
        val response = NativeBridge.nativeDecodeLuma8(pixels.slice(), width, height, rowStride)
        mapNativeStatus(response.status)?.let { throw ScanException(it) }
        return mapNativeResponse(response) ?: throw ScanException(ScanError.INTERNAL)
    }

    /** Decodes one PNG or JPEG. EXIF orientation is normalized before decoding. */
    fun decodeImage(encoded: ByteArray): ScanResult {
        if (encoded.isEmpty() || encoded.size > MAX_IMAGE_BYTES) {
            throw ScanException(ScanError.INVALID_IMAGE)
        }
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(encoded, 0, encoded.size, bounds)
        val width = bounds.outWidth
        val height = bounds.outHeight
        if (width < 2 || height < 2 || width.toLong() * height > MAX_IMAGE_PIXELS) {
            throw ScanException(ScanError.INVALID_IMAGE)
        }
        val decoded = BitmapFactory.decodeByteArray(encoded, 0, encoded.size)
            ?: throw ScanException(ScanError.INVALID_IMAGE)
        val bitmap = orient(decoded, exifOrientation(encoded))
        try {
            val packedSize = Math.multiplyExact(bitmap.width, bitmap.height)
            val luma = ByteBuffer.allocateDirect(packedSize)
            val row = IntArray(bitmap.width)
            for (y in 0 until bitmap.height) {
                bitmap.getPixels(row, 0, bitmap.width, 0, y, bitmap.width, 1)
                for (pixel in row) {
                    val red = pixel shr 16 and 0xff
                    val green = pixel shr 8 and 0xff
                    val blue = pixel and 0xff
                    luma.put(((299 * red + 587 * green + 114 * blue) / 1000).toByte())
                }
            }
            luma.flip()
            return decodeLuma(luma, bitmap.width, bitmap.height, bitmap.width)
        } catch (error: ArithmeticException) {
            throw ScanException(ScanError.INVALID_IMAGE)
        } finally {
            if (bitmap !== decoded) bitmap.recycle()
            decoded.recycle()
        }
    }

    private fun exifOrientation(encoded: ByteArray): Int = try {
        ExifInterface(ByteArrayInputStream(encoded)).getAttributeInt(
            ExifInterface.TAG_ORIENTATION,
            ExifInterface.ORIENTATION_NORMAL,
        )
    } catch (_: Exception) {
        ExifInterface.ORIENTATION_NORMAL
    }

    private fun orient(source: Bitmap, orientation: Int): Bitmap {
        val matrix = Matrix()
        when (orientation) {
            ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> matrix.setScale(-1f, 1f)
            ExifInterface.ORIENTATION_ROTATE_180 -> matrix.setRotate(180f)
            ExifInterface.ORIENTATION_FLIP_VERTICAL -> matrix.setScale(1f, -1f)
            ExifInterface.ORIENTATION_TRANSPOSE -> {
                matrix.setRotate(90f)
                matrix.postScale(-1f, 1f)
            }
            ExifInterface.ORIENTATION_ROTATE_90 -> matrix.setRotate(90f)
            ExifInterface.ORIENTATION_TRANSVERSE -> {
                matrix.setRotate(-90f)
                matrix.postScale(-1f, 1f)
            }
            ExifInterface.ORIENTATION_ROTATE_270 -> matrix.setRotate(-90f)
            else -> return source
        }
        return Bitmap.createBitmap(source, 0, 0, source.width, source.height, matrix, true)
    }

    private companion object {
        const val MAX_IMAGE_BYTES = 8 * 1024 * 1024
        const val MAX_IMAGE_PIXELS = 16_000_000L
    }
}
