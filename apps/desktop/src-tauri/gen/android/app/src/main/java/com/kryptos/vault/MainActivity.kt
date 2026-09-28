package com.kryptos.vault

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.view.View
import android.view.WindowManager
import android.view.autofill.AutofillManager
import androidx.activity.enableEdgeToEdge
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsAnimationCompat
import androidx.core.view.WindowInsetsCompat

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

  fun biometricStatus(): String = Biometric.status(this)

  fun biometricEnable() = Biometric.enable(this, KryptosBridge::biometricDone)

  fun biometricUnlock() = Biometric.unlock(this, KryptosBridge::biometricDone)

  fun biometricDisable() = Biometric.disable(this)

  fun onMasterUnlock() = Biometric.onMasterUnlock(this)
}
