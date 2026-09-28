package com.kryptos.app

import android.app.PendingIntent
import android.app.assist.AssistStructure
import android.content.Intent
import android.os.CancellationSignal
import android.service.autofill.AutofillService
import android.service.autofill.Dataset
import android.service.autofill.FillCallback
import android.service.autofill.FillRequest
import android.service.autofill.FillResponse
import android.service.autofill.SaveCallback
import android.service.autofill.SaveRequest
import android.text.InputType
import android.view.View
import android.view.autofill.AutofillId
import android.view.autofill.AutofillValue
import android.widget.RemoteViews
import org.json.JSONArray

/**
 * System-wide autofill provider. Reads matching logins from the in-memory vault
 * (via JNI) and hands them to Android; nothing leaves the device.
 *
 * Browser pages are matched by web domain, but only when the requesting app is a
 * known browser: any other app could claim an arbitrary webDomain to phish
 * credentials. Native apps only match entries linked to "androidapp://<package>".
 */
class KryptosAutofillService : AutofillService() {

  override fun onFillRequest(request: FillRequest, cancellationSignal: CancellationSignal, callback: FillCallback) {
    val structure = request.fillContexts.lastOrNull()?.structure ?: return callback.onSuccess(null)
    val pkg = structure.activityComponent.packageName
    if (pkg == packageName) return callback.onSuccess(null)

    val fields = Fields().also { it.scan(structure) }
    val ids = listOfNotNull(fields.username, fields.password)
    if (fields.password == null && fields.username == null) return callback.onSuccess(null)

    val domain = fields.webDomain?.takeIf { pkg in TRUSTED_BROWSERS }
    val target = domain?.let { "https://$it" } ?: "androidapp://$pkg"
    val label = domain ?: pkg

    val json = if (KryptosBridge.ensureLoaded()) runCatching { KryptosBridge.fillData(target) }.getOrNull() else null
    val response = FillResponse.Builder()

    if (json == null) {
      // Locked (or app not started): one entry that opens Kryptos to unlock.
      val intent = Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      val sender = PendingIntent.getActivity(
        this, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_CANCEL_CURRENT,
      ).intentSender
      response.setAuthentication(ids.toTypedArray(), sender, row("Sblocca Kryptos", label))
      return callback.onSuccess(response.build())
    }

    val logins = JSONArray(json)
    if (logins.length() == 0) return callback.onSuccess(null)
    for (i in 0 until logins.length()) {
      val l = logins.getJSONObject(i)
      val user = l.optString("username")
      @Suppress("DEPRECATION")
      val ds = Dataset.Builder(row(l.optString("title"), user.ifEmpty { label }))
      fields.username?.let { ds.setValue(it, AutofillValue.forText(user)) }
      fields.password?.let { ds.setValue(it, AutofillValue.forText(l.optString("password"))) }
      response.addDataset(ds.build())
    }
    callback.onSuccess(response.build())
  }

  // Saving new logins from other apps is not supported yet.
  override fun onSaveRequest(request: SaveRequest, callback: SaveCallback) = callback.onSuccess()

  private fun row(title: String, subtitle: String) =
    RemoteViews(packageName, R.layout.autofill_item).apply {
      setTextViewText(R.id.title, title)
      setTextViewText(R.id.subtitle, subtitle)
    }

  /** Finds the username and password fields using autofill hints, input types and HTML attributes. */
  private class Fields {
    var username: AutofillId? = null
    var password: AutofillId? = null
    var webDomain: String? = null

    fun scan(s: AssistStructure) {
      for (i in 0 until s.windowNodeCount) visit(s.getWindowNodeAt(i).rootViewNode)
    }

    private fun visit(n: AssistStructure.ViewNode) {
      n.webDomain?.takeIf { it.isNotBlank() && webDomain == null }?.let { webDomain = it }
      val id = n.autofillId
      if (id != null && n.autofillType == View.AUTOFILL_TYPE_TEXT) {
        when {
          password == null && isPassword(n) -> password = id
          username == null && isUsername(n) -> username = id
        }
      }
      for (i in 0 until n.childCount) visit(n.getChildAt(i))
    }

    private fun hints(n: AssistStructure.ViewNode): List<String> {
      val html = n.htmlInfo?.attributes?.flatMap { listOfNotNull(it.first, it.second) } ?: emptyList()
      return (n.autofillHints?.toList().orEmpty() + listOfNotNull(n.idEntry, n.hint) + html).map { it.lowercase() }
    }

    private fun isPassword(n: AssistStructure.ViewNode): Boolean {
      val variation = n.inputType and InputType.TYPE_MASK_VARIATION
      val pwType = n.inputType and InputType.TYPE_MASK_CLASS == InputType.TYPE_CLASS_TEXT &&
        (variation == InputType.TYPE_TEXT_VARIATION_PASSWORD ||
          variation == InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD ||
          variation == InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD)
      return pwType || hints(n).any { it.contains("password") || it == "current-password" }
    }

    private fun isUsername(n: AssistStructure.ViewNode): Boolean {
      val variation = n.inputType and InputType.TYPE_MASK_VARIATION
      if (variation == InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS ||
        variation == InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS) return true
      return hints(n).any { h -> USER_HINTS.any { h.contains(it) } }
    }
  }

  companion object {
    private val USER_HINTS = listOf("username", "email", "user", "login", "account", "e-mail")

    /** Browsers whose reported webDomain we trust (they populate it from the real page URL). */
    val TRUSTED_BROWSERS = setOf(
      "com.android.chrome", "com.chrome.beta", "com.chrome.dev", "com.chrome.canary",
      "org.mozilla.firefox", "org.mozilla.firefox_beta", "org.mozilla.fenix", "org.mozilla.focus",
      "com.brave.browser", "com.microsoft.emmx", "com.sec.android.app.sbrowser",
      "com.vivaldi.browser", "com.opera.browser", "com.duckduckgo.mobile.android",
      "org.chromium.chrome", "com.kiwibrowser.browser",
    )
  }
}
