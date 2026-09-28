package com.kryptos.vault

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Handler
import android.os.Looper
import androidx.core.content.ContextCompat
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner

/** JNI surface implemented in Rust (src-tauri/src/android.rs). */
object KryptosBridge {
  @Volatile private var ready = false

  /**
   * Loads the Rust library and sets up the shared vault state once per process.
   * The autofill side can start the process before (or without) the UI, so both
   * MainActivity and the autofill classes call this.
   */
  @Synchronized
  fun ensureReady(context: Context): Boolean {
    if (ready) return true
    val app = context.applicationContext
    ready = runCatching {
      System.loadLibrary("kryptos_app_lib")
      init(app.dataDir.absolutePath)
    }.isSuccess
    if (ready) watchVisibility(app)
    return ready
  }

  /** Locks the vault shortly after Kryptos leaves the screen, and at once on screen-off. */
  private fun watchVisibility(app: Context) {
    Handler(Looper.getMainLooper()).post {
      ProcessLifecycleOwner.get().lifecycle.addObserver(object : DefaultLifecycleObserver {
        override fun onStart(owner: LifecycleOwner) = setForeground(true)
        override fun onStop(owner: LifecycleOwner) = setForeground(false)
      })
    }
    val screenOff = object : BroadcastReceiver() {
      override fun onReceive(context: Context, intent: Intent) = screenOff()
    }
    ContextCompat.registerReceiver(app, screenOff, IntentFilter(Intent.ACTION_SCREEN_OFF), ContextCompat.RECEIVER_NOT_EXPORTED)
  }

  @JvmStatic private external fun init(dataDir: String)

  @JvmStatic external fun isUnlocked(): Boolean

  /** JSON array of {title, username, password}, or null if the vault is locked. */
  @JvmStatic external fun fillData(target: String): String?

  @JvmStatic external fun setForeground(visible: Boolean)

  @JvmStatic external fun screenOff()

  /** The raw vault key while unlocked, else null. Callers must wipe it after use. */
  @JvmStatic external fun vaultKey(): ByteArray?

  /** Opens the vault with a key released by the fingerprint prompt. */
  @JvmStatic external fun unlockWithKey(key: ByteArray): Boolean

  /** Completes a prompt the UI asked for (see biometric_prompt in android.rs). */
  @JvmStatic external fun biometricDone(result: String)
}
