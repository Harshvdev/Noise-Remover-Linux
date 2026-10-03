package com.voicecleaner.app

import android.Manifest
import android.content.pm.PackageManager
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

import java.io.File
import java.io.FileOutputStream

class MainActivity : TauriActivity() {
  companion object {
    init {
      try {
        System.loadLibrary("onnxruntime")
        System.loadLibrary("sherpa-onnx-c-api")
      } catch (e: Throwable) {
        android.util.Log.e("VoiceCleaner", "Failed to pre-load sherpa/onnx native libraries: ${e.message}", e)
      }
    }
  }

  override fun onWebViewCreate(webView: android.webkit.WebView) {
    super.onWebViewCreate(webView)
    webView.setBackgroundColor(android.graphics.Color.parseColor("#F5F6F9"))
    webView.settings.setSupportZoom(false)
    webView.settings.builtInZoomControls = false
    webView.settings.displayZoomControls = false
    webView.overScrollMode = android.view.View.OVER_SCROLL_NEVER
    webView.isVerticalScrollBarEnabled = false
    webView.isHorizontalScrollBarEnabled = false
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    val bgLightColor = android.graphics.Color.parseColor("#F5F6F9")
    window.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(bgLightColor))
    window.decorView.setBackgroundColor(bgLightColor)
    window.addFlags(android.view.WindowManager.LayoutParams.FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS)
    window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_TRANSLUCENT_STATUS)
    window.statusBarColor = bgLightColor
    window.navigationBarColor = bgLightColor

    extractModelIfNeeded("models/dpdfnet2_48khz_hr.onnx")
    super.onCreate(savedInstanceState)

    val controller = WindowInsetsControllerCompat(window, window.decorView)
    controller.isAppearanceLightStatusBars = true
    controller.isAppearanceLightNavigationBars = true

    volumeControlStream = android.media.AudioManager.STREAM_MUSIC
    AudioDeviceHelper.init(this)

    val permissionsToRequest = mutableListOf<String>()
    if (ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
      permissionsToRequest.add(Manifest.permission.RECORD_AUDIO)
    }
    if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.S) {
      if (ContextCompat.checkSelfPermission(this, Manifest.permission.BLUETOOTH_CONNECT) != PackageManager.PERMISSION_GRANTED) {
        permissionsToRequest.add(Manifest.permission.BLUETOOTH_CONNECT)
      }
    }
    if (permissionsToRequest.isNotEmpty()) {
      ActivityCompat.requestPermissions(this, permissionsToRequest.toTypedArray(), 1001)
    }
  }

  override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
    super.onRequestPermissionsResult(requestCode, permissions, grantResults)
    if (requestCode == 1001) {
      AudioDeviceHelper.onPermissionsGranted()
    }
  }


  private fun extractModelIfNeeded(assetPath: String) {
    try {
      val modelFile = File(filesDir, assetPath)
      if (!modelFile.exists() || modelFile.length() < 1000000L) {
        modelFile.parentFile?.mkdirs()
        assets.open(assetPath).use { input ->
          FileOutputStream(modelFile).use { output ->
            input.copyTo(output)
          }
        }
        android.util.Log.i("VoiceCleaner", "Extracted model $assetPath to ${modelFile.absolutePath} (${modelFile.length()} bytes)")
      }
    } catch (e: Exception) {
      android.util.Log.w("VoiceCleaner", "Could not extract asset $assetPath: ${e.message}")
    }
  }
}
