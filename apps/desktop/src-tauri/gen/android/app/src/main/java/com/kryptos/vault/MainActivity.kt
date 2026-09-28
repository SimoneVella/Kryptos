package com.kryptos.vault

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // Keep passwords out of screenshots, screen recordings and the recent-apps thumbnail.
    // Debug builds allow screenshots so the UI can be inspected during development.
    if (!BuildConfig.DEBUG) {
      window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
    }
  }
}
