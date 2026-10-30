package com.kryptos.vault

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.view.View
import android.view.WindowManager
import android.view.autofill.AutofillManager
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.content.FileProvider
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsAnimationCompat
import androidx.core.view.WindowInsetsCompat
import java.io.File

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // Keep passwords out of screenshots, screen recordings and the recent-apps thumbnail.
    // Debug builds allow screenshots so the UI can be inspected during development.
    if (!BuildConfig.DEBUG) {
      window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
    }
    followKeyboard(findViewById(android.R.id.content))
    KryptosBridge.ensureReady(this)
  }

  // The UI is a single page, so WebView history is empty and the default back
  // handling would close the app. Ask the UI first (src/lib/back.ts): it closes
  // the open sheet or returns to the passwords tab; otherwise Android goes back.
  override fun onWebViewCreate(webView: WebView) {
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        webView.evaluateJavascript("window.__kryptosBack?.() === true") { handled ->
          if (handled == "true") return@evaluateJavascript
          isEnabled = false
          onBackPressedDispatcher.onBackPressed()
          isEnabled = true
        }
      }
    })
  }

  /**
   * Edge-to-edge windows are not resized for the keyboard, so the WebView would stay
   * full height and scroll under it. Shrink the content by the keyboard height instead,
   * frame by frame during the keyboard animation so the UI moves up together with it.
   */
  private fun followKeyboard(content: View) {
    var animating = false
    val ime = WindowInsetsCompat.Type.ime()
    fun apply(insets: WindowInsetsCompat): WindowInsetsCompat {
      content.setPadding(0, 0, 0, insets.getInsets(ime).bottom)
      // Already handled here: don't let the WebView react to the keyboard a second time.
      return WindowInsetsCompat.Builder(insets).setInsets(ime, Insets.NONE).build()
    }
    ViewCompat.setOnApplyWindowInsetsListener(content) { _, insets ->
      // While animating, onProgress drives the padding; the end state would make it jump.
      if (animating) insets else apply(insets)
    }
    ViewCompat.setWindowInsetsAnimationCallback(
      content,
      object : WindowInsetsAnimationCompat.Callback(DISPATCH_MODE_CONTINUE_ON_SUBTREE) {
        override fun onPrepare(animation: WindowInsetsAnimationCompat) {
          if (animation.typeMask and ime != 0) animating = true
        }

        override fun onProgress(insets: WindowInsetsCompat, running: List<WindowInsetsAnimationCompat>) =
          apply(insets)

        override fun onEnd(animation: WindowInsetsAnimationCompat) {
          if (animation.typeMask and ime == 0) return
          animating = false
          ViewCompat.getRootWindowInsets(content)?.let(::apply)
        }
      },
    )
  }

  // Called from Rust (src-tauri/src/android.rs) on the UI thread.

  /** True only when Kryptos is the selected system autofill service. */
  fun isAutofillEnabled(): Boolean =
    getSystemService(AutofillManager::class.java)?.hasEnabledAutofillServices() == true

  /** Opens the system prompt that makes Kryptos the autofill service. */
  fun openAutofillSettings() {
    val intent = Intent(Settings.ACTION_REQUEST_SET_AUTOFILL_SERVICE, Uri.parse("package:$packageName"))
    runCatching { startActivity(intent) }
      .onFailure { runCatching { startActivity(Intent(Settings.ACTION_SETTINGS)) } }
  }

  /**
   * Cloud sync: the system folder picker (Google Drive, Dropbox... appear there).
   * Kryptos keeps access to that one folder across restarts and nothing else.
   */
  fun pickCloudFolder() {
    val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE)
    launchActivityForResult(intent) { result ->
      val uri = result?.data?.data?.takeIf { result.resultCode == RESULT_OK }
      if (uri != null) {
        val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
        runCatching { contentResolver.takePersistableUriPermission(uri, flags) }
      }
      KryptosBridge.cloudPicked(uri?.toString().orEmpty())
    }
  }

  fun biometricStatus(): String = Biometric.status(this)

  fun biometricEnable() = Biometric.enable(this, KryptosBridge::biometricDone)

  fun biometricUnlock() = Biometric.unlock(this, KryptosBridge::biometricDone)

  fun biometricDisable() = Biometric.disable(this)

  fun onMasterUnlock() = Biometric.onMasterUnlock(this)

  /** Shares an encrypted backup: copies [src] into the cache as [name] and opens the share sheet. */
  fun shareFile(src: String, name: String) {
    val dir = File(cacheDir, "share").apply { deleteRecursively(); mkdirs() }
    val copy = File(dir, name)
    runCatching { File(src).copyTo(copy, overwrite = true) }.onFailure { return }
    val uri = FileProvider.getUriForFile(this, "$packageName.fileprovider", copy)
    val send = Intent(Intent.ACTION_SEND)
      .setType("application/octet-stream")
      .putExtra(Intent.EXTRA_STREAM, uri)
      .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    startActivity(Intent.createChooser(send, null))
  }
}
