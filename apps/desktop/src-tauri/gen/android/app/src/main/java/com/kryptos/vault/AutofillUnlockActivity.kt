package com.kryptos.vault

import android.app.assist.AssistStructure
import android.content.Intent
import android.os.Bundle
import android.service.autofill.Dataset
import android.view.autofill.AutofillManager
import androidx.core.content.IntentCompat
import androidx.fragment.app.FragmentActivity
import com.google.android.material.dialog.MaterialAlertDialogBuilder

/**
 * Opened by the "Unlock Kryptos" suggestion while the vault is locked. Invisible:
 * it shows the fingerprint prompt and, on success, fills the form right away
 * (asking which account first if the site has several), without opening Kryptos.
 * Without a usable fingerprint, or when the user picks the master password, it
 * opens Kryptos instead.
 */
class AutofillUnlockActivity : FragmentActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    if (savedInstanceState != null) return // recreated while the prompt is showing: it reattaches itself
    if (!KryptosBridge.ensureReady(this) || Biometric.status(this) != "on") return openKryptos()
    Biometric.unlock(this) { result ->
      when (result) {
        "ok" -> fill()
        "cancelled", "lockout" -> finish()
        else -> openKryptos()
      }
    }
  }

  private fun fill() {
    val structure = IntentCompat.getParcelableExtra(intent, AutofillManager.EXTRA_ASSIST_STRUCTURE, AssistStructure::class.java)
    val logins = structure?.let { KryptosAutofillService.unlockedDatasets(this, it) }.orEmpty()
    when (logins.size) {
      0 -> finish()
      1 -> reply(logins[0].second)
      else -> MaterialAlertDialogBuilder(this)
        .setTitle(R.string.autofill_choose_account)
        .setItems(logins.map { it.first }.toTypedArray()) { _, i -> reply(logins[i].second) }
        .setOnCancelListener { finish() }
        .show()
    }
  }

  private fun reply(dataset: Dataset) {
    setResult(RESULT_OK, Intent().putExtra(AutofillManager.EXTRA_AUTHENTICATION_RESULT, dataset))
    finish()
  }

  private fun openKryptos() {
    startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    finish()
  }
}
