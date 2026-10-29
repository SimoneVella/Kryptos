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
 * Opened by a Kryptos suggestion. Invisible: it shows the fingerprint prompt and,
 * on success, fills the form right away without opening Kryptos.
 *
 * - Vault locked ("Unlock Kryptos", or a suggestion that went stale): the
 *   fingerprint unlocks it, then fills (asking which account first if the site
 *   has several). Without a usable fingerprint, or when the user picks the master
 *   password, it opens Kryptos instead.
 * - Vault unlocked: the fingerprint confirms this one fill (one prompt per fill).
 */
class AutofillUnlockActivity : FragmentActivity() {
  private val entryId get() = intent.getStringExtra(KryptosAutofillService.EXTRA_ENTRY_ID)

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    if (savedInstanceState != null) return // recreated while the prompt is showing: it reattaches itself
    if (!KryptosBridge.ensureReady(this)) return openKryptos()
    if (KryptosBridge.isUnlocked()) return confirm()
    if (Biometric.status(this) != "on") return openKryptos()
    Biometric.unlock(this) { result ->
      when (result) {
        "ok" -> fill()
        "cancelled", "lockout" -> finish()
        else -> openKryptos()
      }
    }
  }

  private fun confirm() {
    val logins = suggestions()
    if (logins.isEmpty()) return finish()
    // Fingerprint unlock off: the unlocked vault is the only gate, as before.
    if (Biometric.status(this) != "on") return choose(logins)
    val title = logins.singleOrNull()?.let { getString(R.string.biometric_fill_title, it.title) } ?: getString(R.string.app_name)
    Biometric.confirm(this, title) { result ->
      when (result) {
        "ok" -> fill()
        "cancelled", "use_master", "lockout" -> finish()
        else -> openKryptos()
      }
    }
  }

  /** Read again after the prompt: the vault may have locked while it was showing. */
  private fun fill() = choose(suggestions())

  private fun suggestions(): List<KryptosAutofillService.Suggestion> {
    val structure = IntentCompat.getParcelableExtra(intent, AutofillManager.EXTRA_ASSIST_STRUCTURE, AssistStructure::class.java)
    return structure?.let { KryptosAutofillService.unlockedDatasets(this, it, entryId) }.orEmpty()
  }

  private fun choose(logins: List<KryptosAutofillService.Suggestion>) {
    when (logins.size) {
      0 -> finish()
      1 -> reply(logins[0].dataset)
      else -> MaterialAlertDialogBuilder(this)
        .setTitle(R.string.autofill_choose_account)
        .setItems(logins.map { it.label }.toTypedArray()) { _, i -> reply(logins[i].dataset) }
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
