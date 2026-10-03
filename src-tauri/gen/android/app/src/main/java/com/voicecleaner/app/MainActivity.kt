package com.voicecleaner.app

import android.Manifest
import android.content.pm.PackageManager
import android.os.Bundle
import android.view.KeyEvent
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

import java.io.File
import java.io.FileOutputStream

class MainActivity : TauriActivity() {
  private var mWebView: WebView? = null

  inner class AndroidBridge {
    @JavascriptInterface
    fun exitApp() {
      runOnUiThread {
        moveTaskToBack(true)
      }
    }
  }

  companion object {
    private var instance: MainActivity? = null

    @JvmStatic
    fun getInstance(): MainActivity? = instance

    @JvmStatic
    fun exitApp() {
      val activity = instance ?: return
      activity.runOnUiThread {
        activity.moveTaskToBack(true)
      }
    }

    init {
      try {
        System.loadLibrary("onnxruntime")
        System.loadLibrary("sherpa-onnx-c-api")
      } catch (e: Throwable) {
        android.util.Log.e("VoiceCleaner", "Failed to pre-load sherpa/onnx native libraries: ${e.message}", e)
      }
    }

    @JvmStatic
    fun shareTrack(filePath: String, title: String) {
      val activity = instance ?: return
      activity.runOnUiThread {
        try {
          val srcFile = File(filePath)
          if (!srcFile.exists()) {
            android.widget.Toast.makeText(activity, "Audio file not found", android.widget.Toast.LENGTH_SHORT).show()
            return@runOnUiThread
          }

          val safeTitle = title.replace(Regex("[^a-zA-Z0-9._ -]"), "_").trim()
          val filename = if (safeTitle.endsWith(".wav", ignoreCase = true)) safeTitle else "$safeTitle.wav"

          val cacheFile = File(activity.cacheDir, filename)
          srcFile.copyTo(cacheFile, overwrite = true)

          val uri = androidx.core.content.FileProvider.getUriForFile(
            activity,
            "${activity.packageName}.fileprovider",
            cacheFile
          )
          val shareIntent = android.content.Intent(android.content.Intent.ACTION_SEND).apply {
            type = "audio/*"
            putExtra(android.content.Intent.EXTRA_STREAM, uri)
            putExtra(android.content.Intent.EXTRA_SUBJECT, title)
            addFlags(android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
          }
          activity.startActivity(android.content.Intent.createChooser(shareIntent, "Share Audio"))
        } catch (e: Exception) {
          android.util.Log.e("VoiceCleaner", "Share failed: ${e.message}", e)
          android.widget.Toast.makeText(activity, "Share failed: ${e.message}", android.widget.Toast.LENGTH_SHORT).show()
        }
      }
    }

    @JvmStatic
    fun exportTrack(filePath: String, title: String): Boolean {
      val activity = instance ?: return false
      activity.runOnUiThread {
        try {
          val srcFile = File(filePath)
          if (!srcFile.exists()) {
            android.widget.Toast.makeText(activity, "Audio file not found", android.widget.Toast.LENGTH_SHORT).show()
            return@runOnUiThread
          }

          val safeTitle = title.replace(Regex("[^a-zA-Z0-9._ -]"), "_").trim()
          val filename = if (safeTitle.endsWith(".wav", ignoreCase = true)) safeTitle else "$safeTitle.wav"

          var exportedUri: android.net.Uri? = null

          if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.Q) {
            val resolver = activity.contentResolver
            val values = android.content.ContentValues().apply {
              put(android.provider.MediaStore.Audio.Media.DISPLAY_NAME, filename)
              put(android.provider.MediaStore.Audio.Media.MIME_TYPE, "audio/wav")
              put(android.provider.MediaStore.Audio.Media.RELATIVE_PATH, "Music/VoiceCleaner")
              put(android.provider.MediaStore.Audio.Media.IS_PENDING, 1)
            }
            val collection = android.provider.MediaStore.Audio.Media.getContentUri(android.provider.MediaStore.VOLUME_EXTERNAL_PRIMARY)
            val itemUri = resolver.insert(collection, values)
            if (itemUri != null) {
              resolver.openOutputStream(itemUri)?.use { outStream ->
                srcFile.inputStream().use { inStream ->
                  inStream.copyTo(outStream)
                }
              }
              values.clear()
              values.put(android.provider.MediaStore.Audio.Media.IS_PENDING, 0)
              resolver.update(itemUri, values, null, null)
              exportedUri = itemUri
            }
          } else {
            val musicDir = android.os.Environment.getExternalStoragePublicDirectory(android.os.Environment.DIRECTORY_MUSIC)
            val appMusicDir = File(musicDir, "VoiceCleaner")
            appMusicDir.mkdirs()
            val destFile = File(appMusicDir, filename)
            srcFile.copyTo(destFile, overwrite = true)
            android.media.MediaScannerConnection.scanFile(
              activity,
              arrayOf(destFile.absolutePath),
              arrayOf("audio/wav")
            ) { _, _ -> }
            exportedUri = androidx.core.content.FileProvider.getUriForFile(
              activity,
              "${activity.packageName}.fileprovider",
              destFile
            )
          }

          android.widget.Toast.makeText(
            activity,
            "Saved to Music/VoiceCleaner/$filename",
            android.widget.Toast.LENGTH_LONG
          ).show()

          if (exportedUri != null) {
            val viewIntent = android.content.Intent(android.content.Intent.ACTION_VIEW).apply {
              setDataAndType(exportedUri, "audio/*")
              addFlags(android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            try {
              activity.startActivity(android.content.Intent.createChooser(viewIntent, "Open with"))
            } catch (_: Exception) {}
          }
        } catch (e: Exception) {
          android.util.Log.e("VoiceCleaner", "Export failed: ${e.message}", e)
          android.widget.Toast.makeText(activity, "Export failed: ${e.message}", android.widget.Toast.LENGTH_SHORT).show()
        }
      }
      return true
    }
  }

  override fun onWebViewCreate(webView: WebView) {
    mWebView = webView
    super.onWebViewCreate(webView)
    webView.setBackgroundColor(android.graphics.Color.parseColor("#F5F6F9"))
    webView.settings.setSupportZoom(false)
    webView.settings.builtInZoomControls = false
    webView.settings.displayZoomControls = false
    webView.overScrollMode = android.view.View.OVER_SCROLL_NEVER
    webView.isVerticalScrollBarEnabled = false
    webView.isHorizontalScrollBarEnabled = false
    webView.settings.domStorageEnabled = true
    webView.settings.databaseEnabled = true
    webView.settings.mediaPlaybackRequiresUserGesture = false
    webView.addJavascriptInterface(AndroidBridge(), "AndroidBridge")
  }

  override fun onKeyDown(keyCode: Int, event: KeyEvent?): Boolean {
    if (keyCode == KeyEvent.KEYCODE_BACK) {
      mWebView?.evaluateJavascript(
        "if (window.__handleAndroidBack) { window.__handleAndroidBack(); } else { window.history.back(); }",
        null
      )
      return true
    }
    return super.onKeyDown(keyCode, event)
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    instance = this
    val bgLightColor = android.graphics.Color.parseColor("#F5F6F9")
    window.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(bgLightColor))
    window.decorView.setBackgroundColor(bgLightColor)
    window.addFlags(android.view.WindowManager.LayoutParams.FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS)
    window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_TRANSLUCENT_STATUS)
    window.statusBarColor = bgLightColor
    window.navigationBarColor = bgLightColor

    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        mWebView?.evaluateJavascript(
          "if (window.__handleAndroidBack) { window.__handleAndroidBack(); } else { window.history.back(); }",
          null
        )
      }
    })

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
