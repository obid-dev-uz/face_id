package kho.face.camera

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import android.util.Log
import android.util.Size
import android.view.WindowManager
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.resolutionselector.AspectRatioStrategy
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleOwner
import app.tauri.PermissionState
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.nio.ByteBuffer
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

@InvokeArg
class SecureArgs {
    var enabled: Boolean = false
}

@InvokeArg
class StartArgs {
    var facing: String = "front"
    var width: Int = 640
    var height: Int = 480
}

@TauriPlugin(permissions = [Permission(strings = [Manifest.permission.CAMERA], alias = "camera")])
class CameraPlugin(private val activity: Activity) : Plugin(activity) {
    private var provider: ProcessCameraProvider? = null
    private var analyzerExecutor: ExecutorService? = null

    /** Rust: `Java_kho_face_camera_CameraPlugin_nativeOnFrame` (plugins/khoface-camera/src/android.rs). */
    private external fun nativeOnFrame(
        y: ByteBuffer, u: ByteBuffer, v: ByteBuffer,
        width: Int, height: Int, yStride: Int, uvStride: Int, uvPixelStride: Int,
        rotation: Int, front: Boolean,
    )

    private fun withProvider(invoke: Invoke, block: (ProcessCameraProvider) -> Unit) {
        val future = ProcessCameraProvider.getInstance(activity)
        future.addListener({
            try {
                val p = future.get()
                provider = p
                block(p)
            } catch (e: Exception) {
                Log.e(TAG, "CameraX xatosi", e)
                invoke.reject(e.message ?: e.toString())
            }
        }, ContextCompat.getMainExecutor(activity))
    }

    @Command
    fun availability(invoke: Invoke) {
        withProvider(invoke) { p ->
            val r = JSObject()
            r.put("front", p.hasCamera(CameraSelector.DEFAULT_FRONT_CAMERA))
            r.put("back", p.hasCamera(CameraSelector.DEFAULT_BACK_CAMERA))
            invoke.resolve(r)
        }
    }

    @Command
    fun start(invoke: Invoke) {
        if (getPermissionState("camera") != PermissionState.GRANTED) {
            invoke.reject("Kamera ruxsati berilmagan", "PERMISSION_DENIED")
            return
        }
        val args = invoke.parseArgs(StartArgs::class.java)
        val front = args.facing != "back"
        withProvider(invoke) { p ->
            val selector = if (front) CameraSelector.DEFAULT_FRONT_CAMERA else CameraSelector.DEFAULT_BACK_CAMERA
            if (!p.hasCamera(selector)) {
                invoke.reject("Kamera topilmadi", "NO_CAMERA")
                return@withProvider
            }
            // Eski (Camera2 LEGACY) qurilmalarda aniq o'lcham bo'lmasa, eng yaqini olinadi.
            val resolution = ResolutionSelector.Builder()
                .setAspectRatioStrategy(AspectRatioStrategy.RATIO_4_3_FALLBACK_AUTO_STRATEGY)
                .setResolutionStrategy(
                    ResolutionStrategy(
                        Size(args.width, args.height),
                        ResolutionStrategy.FALLBACK_RULE_CLOSEST_HIGHER_THEN_LOWER,
                    )
                )
                .build()
            val analysis = ImageAnalysis.Builder()
                .setResolutionSelector(resolution)
                .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                .setOutputImageFormat(ImageAnalysis.OUTPUT_IMAGE_FORMAT_YUV_420_888)
                .build()
            val executor = analyzerExecutor ?: Executors.newSingleThreadExecutor().also { analyzerExecutor = it }
            analysis.setAnalyzer(executor) { img ->
                try {
                    val pl = img.planes
                    nativeOnFrame(
                        pl[0].buffer, pl[1].buffer, pl[2].buffer,
                        img.width, img.height,
                        pl[0].rowStride, pl[1].rowStride, pl[1].pixelStride,
                        img.imageInfo.rotationDegrees, front,
                    )
                } catch (e: Throwable) {
                    Log.e(TAG, "kadrni uzatishda xato", e)
                } finally {
                    img.close()
                }
            }
            p.unbindAll()
            // Lifecycle'ga bog'langan: ilova fonga o'tsa kamera o'zi to'xtaydi, qaytsa davom etadi.
            p.bindToLifecycle(activity as LifecycleOwner, selector, analysis)
            invoke.resolve()
        }
    }

    @Command
    fun stop(invoke: Invoke) {
        activity.runOnUiThread {
            provider?.unbindAll()
            invoke.resolve()
        }
    }

    /** FLAG_SECURE: skrinshot, ekran yozuvi va "so'nggi ilovalar" eskizida ilova ko'rinmaydi. */
    @Command
    fun setSecure(invoke: Invoke) {
        val args = invoke.parseArgs(SecureArgs::class.java)
        activity.runOnUiThread {
            if (args.enabled) {
                activity.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
            } else {
                activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
            }
            invoke.resolve()
        }
    }

    @Command
    fun openSettings(invoke: Invoke) {
        val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", activity.packageName, null))
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        activity.startActivity(intent)
        invoke.resolve()
    }

    override fun onDestroy() {
        provider?.unbindAll()
        analyzerExecutor?.shutdown()
        analyzerExecutor = null
    }

    companion object {
        private const val TAG = "KHOFaceCamera"
    }
}
