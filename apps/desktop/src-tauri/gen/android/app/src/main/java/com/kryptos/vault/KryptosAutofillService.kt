package com.kryptos.vault

import android.app.PendingIntent
import android.app.assist.AssistStructure
import android.content.Context
import android.content.Intent
import android.graphics.BlendMode
import android.graphics.drawable.Icon
import android.os.Build
import android.os.CancellationSignal
import android.service.autofill.AutofillService
import android.service.autofill.Dataset
import android.service.autofill.FillCallback
import android.service.autofill.FillRequest
import android.service.autofill.FillResponse
import android.service.autofill.InlinePresentation
import android.service.autofill.SaveCallback
import android.service.autofill.SaveRequest
import android.text.InputType
import android.view.View
import android.view.autofill.AutofillId
import android.view.autofill.AutofillValue
import android.view.inputmethod.InlineSuggestionsRequest
import android.widget.RemoteViews
import androidx.annotation.RequiresApi
import androidx.autofill.inline.UiVersions
import androidx.autofill.inline.v1.InlineSuggestionUi
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
    val inline = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) request.inlineSuggestionsRequest else null
    callback.onSuccess(buildResponse(this, structure, inline))
  }

  // Saving new logins from other apps is not supported yet.
  override fun onSaveRequest(request: SaveRequest, callback: SaveCallback) = callback.onSuccess()

  companion object {
    /**
     * Suggestions for the login form in [structure]: the matching logins, or a single
     * "Unlock Kryptos" entry while the vault is locked, or null when there is nothing
     * to offer.
     */
    fun buildResponse(ctx: Context, structure: AssistStructure, inlineRequest: InlineSuggestionsRequest?): FillResponse? {
      val form = Form.of(ctx, structure) ?: return null
      val inline = inlineRequest?.let { Inline(ctx, it) }
      val logins = if (KryptosBridge.ensureReady(ctx)) form.logins() else null
      val response = FillResponse.Builder()

      if (logins == null) {
        // Locked: one entry that asks for the fingerprint (AutofillUnlockActivity) and
        // then fills the form directly. The framework adds the form to this intent,
        // hence mutable; it is explicit, so nobody else can receive it.
        val intent = Intent(ctx, AutofillUnlockActivity::class.java)
        val sender = PendingIntent.getActivity(
          ctx, 0, intent, PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_CANCEL_CURRENT,
        ).intentSender
        val title = ctx.getString(R.string.autofill_unlock)
        @Suppress("DEPRECATION")
        val ds = Dataset.Builder(row(ctx, title, form.label))
        // No values yet: they come back from the unlock activity.
        @Suppress("DEPRECATION")
        form.ids.forEach { ds.setValue(it, null) }
        ds.setAuthentication(sender)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
          @Suppress("DEPRECATION")
          inline?.presentation(0, title, form.label)?.let { ds.setInlinePresentation(it) }
        }
        return response.addDataset(ds.build()).build()
      }

      if (logins.isEmpty()) return null
      logins.forEachIndexed { i, login ->
        val ds = form.dataset(ctx, login)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
          @Suppress("DEPRECATION")
          inline?.presentation(i, login.title, login.subtitle(form))?.let { ds.setInlinePresentation(it) }
        }
        response.addDataset(ds.build())
      }
      return response.build()
    }

    /** After a fingerprint unlock: the matching logins as ready-to-fill datasets, with labels. */
    fun unlockedDatasets(ctx: Context, structure: AssistStructure): List<Pair<String, Dataset>> {
      val form = Form.of(ctx, structure) ?: return emptyList()
      return form.logins().orEmpty().map { "${it.title} · ${it.subtitle(form)}" to form.dataset(ctx, it).build() }
    }

    private fun row(ctx: Context, title: String, subtitle: String) =
      RemoteViews(ctx.packageName, R.layout.autofill_item).apply {
        setTextViewText(R.id.title, title)
        setTextViewText(R.id.subtitle, subtitle)
      }

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

  private class Login(val title: String, val username: String, val password: String) {
    fun subtitle(form: Form) = username.ifEmpty { form.label }
  }

  /** A login form another app or a browser page is showing. */
  private class Form(val ids: List<AutofillId>, val fields: Fields, val target: String, val label: String) {
    /** Matching logins, or null while the vault is locked. */
    fun logins(): List<Login>? {
      val json = runCatching { KryptosBridge.fillData(target) }.getOrNull() ?: return null
      val arr = JSONArray(json)
      return (0 until arr.length()).map { i ->
        arr.getJSONObject(i).let { Login(it.optString("title"), it.optString("username"), it.optString("password")) }
      }
    }

    @Suppress("DEPRECATION")
    fun dataset(ctx: Context, login: Login) = Dataset.Builder(row(ctx, login.title, login.subtitle(this))).apply {
      fields.username?.let { setValue(it, AutofillValue.forText(login.username)) }
      fields.password?.let { setValue(it, AutofillValue.forText(login.password)) }
    }

    companion object {
      fun of(ctx: Context, structure: AssistStructure): Form? {
        val pkg = structure.activityComponent.packageName
        if (pkg == ctx.packageName) return null
        val fields = Fields().also { it.scan(structure) }
        val ids = listOfNotNull(fields.username, fields.password)
        if (ids.isEmpty()) return null
        val domain = fields.webDomain?.takeIf { pkg in TRUSTED_BROWSERS }
        return Form(ids, fields, domain?.let { "https://$it" } ?: "androidapp://$pkg", domain ?: pkg)
      }
    }
  }

  /**
   * Suggestion chips shown inside the keyboard (Gboard & co., Android 11+). The keyboard
   * says how many it can show and how they should look; without them Android falls back
   * to the dropdown under the field.
   */
  private class Inline(private val ctx: Context, private val request: InlineSuggestionsRequest) {
    // Required by the inline UI (long-press on a chip); opens Kryptos.
    private val attribution = PendingIntent.getActivity(
      ctx, 1, Intent(ctx, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
    )

    @RequiresApi(Build.VERSION_CODES.R)
    fun presentation(index: Int, title: String, subtitle: String): InlinePresentation? {
      val specs = request.inlinePresentationSpecs
      if (index >= request.maxSuggestionCount || specs.isEmpty()) return null
      val spec = specs[minOf(index, specs.size - 1)]
      if (!UiVersions.getVersions(spec.style).contains(UiVersions.INLINE_UI_VERSION_1)) return null
      val slice = InlineSuggestionUi.newContentBuilder(attribution)
        .setTitle(title)
        .setSubtitle(subtitle)
        // Keyboards tint chip icons to one color; DST keeps the logo's own colors.
        .setStartIcon(Icon.createWithResource(ctx, R.mipmap.ic_launcher).setTintBlendMode(BlendMode.DST))
        .build().slice
      return InlinePresentation(slice, spec, false)
    }
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
}
