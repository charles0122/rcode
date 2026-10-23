package com.rcode.scanner

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.graphics.Matrix
import android.view.Surface
import androidx.annotation.MainThread
import androidx.camera.core.Camera
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.camera.view.TransformExperimental
import androidx.camera.view.transform.CoordinateTransform
import androidx.camera.view.transform.ImageProxyTransformFactory
import androidx.core.content.ContextCompat
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicLong

/** CameraX scanner for one rear-camera RCode at a time. Permission requests remain app-owned. */
@TransformExperimental
class RCodeScanner(
    context: Context,
    private val lifecycleOwner: LifecycleOwner,
    private val previewView: PreviewView,
    private val onResult: (ScanResult) -> Unit,
    private val onError: (ScanError) -> Unit = {},
) : DefaultLifecycleObserver, AutoCloseable {
    private val appContext = context.applicationContext
    private val mainExecutor = ContextCompat.getMainExecutor(appContext)
    private val analysisExecutor: ExecutorService = Executors.newSingleThreadExecutor()
    // Decoder landmarks use the original, unrotated full-buffer coordinates.
    private val transformFactory = ImageProxyTransformFactory()
    private val oneShotGate = OneShotGate()
    private val decoder = RCodeDecoder()
    private val session = AtomicLong(0)

    @Volatile private var requested = false
    @Volatile private var closed = false
    private var bindPending = false
    private var cameraProvider: ProcessCameraProvider? = null
    private var boundCamera: Camera? = null
    private var preview: Preview? = null
    private var analysis: ImageAnalysis? = null

    init {
        lifecycleOwner.lifecycle.addObserver(this)
    }

    fun permissionState(): CameraPermissionState =
        if (ContextCompat.checkSelfPermission(appContext, Manifest.permission.CAMERA) ==
            PackageManager.PERMISSION_GRANTED
        ) CameraPermissionState.GRANTED else CameraPermissionState.REQUIRED

    /** Returns false if the host must request camera permission before starting. */
    @MainThread
    fun start(): Boolean {
        if (closed) return false
        if (permissionState() != CameraPermissionState.GRANTED) {
            reportError(ScanError.PERMISSION_REQUIRED)
            return false
        }
        if (!requested) {
            requested = true
            oneShotGate.reset()
            session.incrementAndGet()
        }
        bindIfReady()
        return true
    }

    @MainThread
    fun stop() {
        requested = false
        session.incrementAndGet()
        unbind()
    }

    /** Returns false when no camera is bound or it does not have a flash unit. */
    @MainThread
    fun setTorch(enabled: Boolean): Boolean {
        val camera = boundCamera ?: return false
        if (!camera.cameraInfo.hasFlashUnit()) return false
        camera.cameraControl.enableTorch(enabled)
        return true
    }

    override fun onStart(owner: LifecycleOwner) {
        bindIfReady()
    }

    override fun onStop(owner: LifecycleOwner) {
        session.incrementAndGet()
        unbind()
    }

    override fun onDestroy(owner: LifecycleOwner) {
        close()
    }

    @MainThread
    override fun close() {
        if (closed) return
        closed = true
        stop()
        lifecycleOwner.lifecycle.removeObserver(this)
        analysisExecutor.shutdown()
    }

    @MainThread
    private fun bindIfReady() {
        if (closed || !requested || permissionState() != CameraPermissionState.GRANTED ||
            !lifecycleOwner.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED) ||
            boundCamera != null || bindPending
        ) return

        bindPending = true
        val providerFuture = ProcessCameraProvider.getInstance(appContext)
        providerFuture.addListener({
            bindPending = false
            if (closed || !requested ||
                !lifecycleOwner.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)
            ) return@addListener
            try {
                val provider = providerFuture.get()
                cameraProvider = provider
                val rotation = previewView.display?.rotation ?: Surface.ROTATION_0
                val previewUseCase = Preview.Builder().setTargetRotation(rotation).build().also {
                    it.surfaceProvider = previewView.surfaceProvider
                }
                val analysisUseCase = ImageAnalysis.Builder()
                    .setTargetRotation(rotation)
                    .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                    .build()
                analysisUseCase.setAnalyzer(analysisExecutor, ::analyze)
                boundCamera = provider.bindToLifecycle(
                    lifecycleOwner,
                    CameraSelector.DEFAULT_BACK_CAMERA,
                    previewUseCase,
                    analysisUseCase,
                )
                preview = previewUseCase
                analysis = analysisUseCase
            } catch (_: Exception) {
                reportError(ScanError.CAMERA_UNAVAILABLE)
            }
        }, mainExecutor)
    }

    @MainThread
    private fun unbind() {
        val provider = cameraProvider
        val previewUseCase = preview
        val analysisUseCase = analysis
        if (provider != null && previewUseCase != null && analysisUseCase != null) {
            provider.unbind(previewUseCase, analysisUseCase)
        }
        boundCamera = null
        preview = null
        analysis = null
    }

    private fun analyze(image: ImageProxy) {
        try {
            if (!requested || closed || oneShotGate.isClaimed()) return
            val frameSession = session.get()
            val yPlane = image.planes.firstOrNull() ?: return
            val luma = yPlane.buffer.duplicate().slice()
            val decoded = decoder.decodeLuma(
                luma,
                image.width,
                image.height,
                yPlane.rowStride,
            )
            val transformed = mapToPreview(decoded, image) ?: return
            if (oneShotGate.claim()) {
                mainExecutor.execute {
                    if (requested && !closed && session.get() == frameSession) {
                        onResult(transformed)
                    } else if (requested && !closed) {
                        // A lifecycle pause invalidated the frame before delivery. Rearm so the
                        // still-requested scan can continue after the lifecycle resumes.
                        oneShotGate.reset()
                    }
                }
            }
        } catch (error: ScanException) {
            if (error.error != ScanError.NO_CODE) reportError(error.error)
        } catch (_: Exception) {
            reportError(ScanError.INTERNAL)
        } finally {
            image.close()
        }
    }

    private fun mapToPreview(result: ScanResult, image: ImageProxy): ScanResult? {
        val target = previewView.outputTransform ?: return null
        val source = transformFactory.getOutputTransform(image)
        val coordinateTransform = CoordinateTransform(source, target)
        val matrix = Matrix()
        coordinateTransform.transform(matrix)
        val values = FloatArray(9)
        matrix.getValues(values)
        val mapped = PreviewCoordinates.map(result.landmarks, values) ?: return null
        return result.copy(landmarks = mapped)
    }

    private fun reportError(error: ScanError) {
        mainExecutor.execute {
            if (!closed) onError(error)
        }
    }
}
