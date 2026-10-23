package com.rcode.rcode_flutter

import android.Manifest
import android.app.Activity
import android.content.pm.PackageManager
import android.view.View
import android.widget.FrameLayout
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleOwner
import androidx.camera.view.PreviewView
import com.rcode.scanner.RCodeScanner
import com.rcode.scanner.RCodeDecoder
import com.rcode.scanner.ScanException
import com.rcode.scanner.ScanError
import com.rcode.scanner.ScanResult
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.embedding.engine.plugins.activity.ActivityAware
import io.flutter.embedding.engine.plugins.activity.ActivityPluginBinding
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import io.flutter.plugin.common.StandardMessageCodec
import io.flutter.plugin.common.PluginRegistry
import io.flutter.plugin.platform.PlatformView
import io.flutter.plugin.platform.PlatformViewFactory

class RcodeFlutterPlugin : FlutterPlugin, MethodChannel.MethodCallHandler, ActivityAware,
    PluginRegistry.RequestPermissionsResultListener {
    private lateinit var messenger: io.flutter.plugin.common.BinaryMessenger
    private lateinit var methods: MethodChannel
    private var activity: Activity? = null
    private var binding: ActivityPluginBinding? = null
    private val views = mutableMapOf<Int, ScannerView>()
    private var permissionResult: MethodChannel.Result? = null
    private val permissionRequestCode = 7403

    override fun onAttachedToEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        messenger = binding.binaryMessenger
        methods = MethodChannel(messenger, "rcode_flutter/methods")
        methods.setMethodCallHandler(this)
        binding.platformViewRegistry.registerViewFactory("rcode_flutter/preview", PreviewFactory())
    }

    override fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        val args = call.arguments as? Map<*, *>
        val view = (args?.get("viewId") as? Int)?.let(views::get)
        when (call.method) {
            "decodeImage" -> {
                val bytes = call.arguments as? ByteArray
                if (bytes == null) { result.error("invalidImage", "Expected encoded image bytes", null); return }
                try { result.success(resultMap(RCodeDecoder().decodeImage(bytes))) }
                catch (error: ScanException) { result.error(errorCode(error.error), error.message, null) }
                catch (error: Exception) { result.error("internal", error.message, null) }
            }
            "permissionState" -> result.success(if (hasPermission()) 0 else 1)
            "requestPermission" -> requestPermission(result)
            "start" -> if (view == null) result.error("not_ready", "Preview is not ready", null)
                else { view.start(); result.success(null) }
            "stop" -> { view?.stop(); result.success(null) }
            "setTorch" -> result.success(if (view?.setTorch(args?.get("enabled") as? Boolean ?: false) == true) 1 else 0)
            else -> result.notImplemented()
        }
    }

    private fun hasPermission() = activity?.let {
        ContextCompat.checkSelfPermission(it, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
    } == true

    private fun requestPermission(result: MethodChannel.Result) {
        val host = activity
        if (host == null) { result.success(1); return }
        if (hasPermission()) { result.success(0); return }
        if (permissionResult != null) {
            result.error("permission_in_progress", "A camera permission request is already active", null)
            return
        }
        permissionResult = result
        ActivityCompat.requestPermissions(host, arrayOf(Manifest.permission.CAMERA), permissionRequestCode)
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray): Boolean {
        if (requestCode != permissionRequestCode) return false
        permissionResult?.success(if (grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED) 0 else 1)
        permissionResult = null
        return true
    }

    override fun onAttachedToActivity(binding: ActivityPluginBinding) {
        activity = binding.activity
        this.binding = binding
        binding.addRequestPermissionsResultListener(this)
    }
    override fun onDetachedFromActivityForConfigChanges() = detachActivity()
    override fun onReattachedToActivityForConfigChanges(binding: ActivityPluginBinding) = onAttachedToActivity(binding)
    override fun onDetachedFromActivity() = detachActivity()
    private fun detachActivity() {
        binding?.removeRequestPermissionsResultListener(this)
        permissionResult?.success(1)
        permissionResult = null
        binding = null
        activity = null
    }

    override fun onDetachedFromEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        methods.setMethodCallHandler(null)
        views.values.toList().forEach(ScannerView::dispose)
        views.clear()
    }

    private inner class PreviewFactory : PlatformViewFactory(StandardMessageCodec.INSTANCE) {
        override fun create(context: android.content.Context, viewId: Int, args: Any?): PlatformView {
            val host = activity as? LifecycleOwner
                ?: throw IllegalStateException("FlutterActivity must implement LifecycleOwner")
            return ScannerView(context, host, viewId).also { views[viewId] = it }
        }
    }

    private inner class ScannerView(context: android.content.Context, owner: LifecycleOwner, private val id: Int) : PlatformView, EventChannel.StreamHandler {
        private val preview = PreviewView(context)
        private val container = FrameLayout(context).apply { addView(preview, FrameLayout.LayoutParams(-1, -1)) }
        private val events = EventChannel(messenger, "rcode_flutter/events/$id")
        private var sink: EventChannel.EventSink? = null
        private val scanner = RCodeScanner(context, owner, preview, ::emitResult, ::emitError)
        init { events.setStreamHandler(this) }
        override fun getView(): View = container
        override fun onListen(arguments: Any?, events: EventChannel.EventSink?) { sink = events }
        override fun onCancel(arguments: Any?) { sink = null }
        fun start(): Boolean = scanner.start()
        fun stop() = scanner.stop()
        fun setTorch(enabled: Boolean) = scanner.setTorch(enabled)
        override fun dispose() { scanner.close(); events.setStreamHandler(null); views.remove(id) }
        private fun emitError(error: ScanError) {
            val code = when (error) {
                ScanError.PERMISSION_REQUIRED -> "permissionRequired"
                ScanError.CAMERA_UNAVAILABLE -> "cameraUnavailable"
                ScanError.NO_CODE -> "noCode"
                ScanError.INVALID_ARGUMENT -> "invalidArgument"
                ScanError.CORRUPT_CODE -> "corruptCode"
                ScanError.INVALID_IMAGE -> "invalidImage"
                ScanError.INTERNAL -> "internal"
            }
            sink?.success(mapOf("type" to "error", "code" to code))
        }
        private fun emitResult(result: ScanResult) {
            sink?.success(resultMap(result) + ("type" to "result"))
        }
    }

    private fun resultMap(result: ScanResult): Map<String, Any> =
        mapOf("payload" to result.payload.map { it.toInt() and 255 },
                "correctedBytes" to result.correctedBytes, "landmarks" to result.landmarks.map { listOf(it.x.toDouble(), it.y.toDouble()) },
                "imageWidth" to result.imageWidth, "imageHeight" to result.imageHeight,
                "spokes" to result.profile.spokes, "errorCorrection" to result.profile.ecc.id)

    private fun errorCode(error: ScanError): String = when (error) {
        ScanError.PERMISSION_REQUIRED -> "permissionRequired"
        ScanError.CAMERA_UNAVAILABLE -> "cameraUnavailable"
        ScanError.NO_CODE -> "noCode"
        ScanError.INVALID_ARGUMENT -> "invalidArgument"
        ScanError.CORRUPT_CODE -> "corruptCode"
        ScanError.INVALID_IMAGE -> "invalidImage"
        ScanError.INTERNAL -> "internal"
    }
}
