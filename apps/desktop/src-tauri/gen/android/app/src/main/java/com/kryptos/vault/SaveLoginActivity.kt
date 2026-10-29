package com.kryptos.vault

import android.content.Intent
import android.os.Bundle
import android.widget.Toast
import androidx.fragment.app.FragmentActivity

/**
 * Opened by Android after the user accepted "Save to Kryptos?" while the vault was
 * locked. Invisible: the fingerprint opens the vault and the waiting login is
 * saved. Without a usable fingerprint it opens Kryptos, which saves the login once
 * the master password unlocks it (within a few minutes, see android.rs).
 */
class SaveLoginActivity : FragmentActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    if (savedInstanceState != null) return // recreated while the prompt is showing: it reattaches itself
    if (!KryptosBridge.ensureReady(this)) return finish()
    if (KryptosBridge.isUnlocked()) return save()
    if (Biometric.status(this) != "on") return openKryptos()
    Biometric.unlock(this) { result ->
      when (result) {
        "ok" -> save()
        "cancelled", "lockout" -> finish()
        else -> openKryptos()
      }
    }
  }

  private fun save() {
    when (KryptosBridge.applyPendingSave()) {
      "added" -> toast(R.string.autofill_saved)
      "updated" -> toast(R.string.autofill_updated)
    }
    finish()
  }

  private fun openKryptos() {
    toast(R.string.autofill_save_unlock)
    startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    finish()
  }

  private fun toast(res: Int) = Toast.makeText(applicationContext, res, Toast.LENGTH_SHORT).show()
}
