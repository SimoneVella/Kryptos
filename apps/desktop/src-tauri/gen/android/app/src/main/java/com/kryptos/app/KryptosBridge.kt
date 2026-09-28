package com.kryptos.app

/** JNI surface implemented in Rust (src-tauri/src/android.rs). */
object KryptosBridge {
  @Volatile private var loaded = false

  /** The Autofill service can start before the UI, so load the Rust library ourselves. */
  fun ensureLoaded(): Boolean {
    if (!loaded) {
      loaded = runCatching { System.loadLibrary("kryptos_app_lib") }.isSuccess
    }
    return loaded
  }

  @JvmStatic external fun isUnlocked(): Boolean

  /** JSON array of {title, username, password}, or null if the vault is locked. */
  @JvmStatic external fun fillData(target: String): String?
}
