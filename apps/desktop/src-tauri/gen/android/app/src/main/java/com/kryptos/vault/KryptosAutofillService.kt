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
import android.service.autofill.SaveInfo
import android.service.autofill.SaveRequest
import android.text.InputType
import android.view.View
import android.view.autofill.AutofillId
import android.view.autofill.AutofillValue
import android.view.inputmethod.InlineSuggestionsRequest
import android.widget.RemoteViews
import android.widget.Toast
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
 *
 * Every login form also gets a SaveInfo, so after the user types a new password
 * Android offers "Save to Kryptos?" (onSaveRequest).
 */
class KryptosAutofillService : AutofillService() {

  override fun onFillRequest(request: FillRequest, cancellationSignal: CancellationSignal, callback: FillCallback) {
    val structure = request.fillContexts.lastOrNull()?.structure ?: return callback.onSuccess(null)
    val inline = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) request.inlineSuggestionsRequest else null
    callback.onSuccess(buildResponse(this, structure, inline))
  }

  /**
   * The user accepted Android's save sheet. Saved at once if the vault is open;
   * otherwise the login waits in memory (Rust) while SaveLoginActivity asks for
   * the fingerprint, or until Kryptos is unlocked with the master password.
   */
  override fun onSaveRequest(request: SaveRequest, callback: SaveCallback) {
    val login = Typed.of(this, request.fillContexts.map { it.structure })
    if (login == null || !KryptosBridge.ensureReady(this)) return callback.onSuccess()
    when (KryptosBridge.saveLogin(login.target, login.title, login.username, login.password)) {
      "added" -> toast(R.string.autofill_saved)
      "updated" -> toast(R.string.autofill_updated)
      "locked" -> if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
        val intent = Intent(this, SaveLoginActivity::class.java)
        val sender = PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        return callback.onSuccess(sender.intentSender)
      } else {
        toast(R.string.autofill_save_unlock)
      }
      "unchanged" -> {}
      else -> return callback.onFailure(getString(R.string.autofill_save_failed))
    }
    callback.onSuccess()
  }

  private fun toast(res: Int) = Toast.makeText(this, res, Toast.LENGTH_SHORT).show()

  companion object {
    /** Set on a suggestion's unlock intent: the entry it stands for. */
    const val EXTRA_ENTRY_ID = "com.kryptos.vault.ENTRY_ID"

    /**
     * Suggestions for the login form in [structure]: the matching logins, or a single
     * "Unlock Kryptos" entry while the vault is locked, or null when there is nothing
     * to offer.
     *
     * With fingerprint unlock on, the logins carry no values: each one asks for the
     * fingerprint (AutofillUnlockActivity) and only then gets its username and
     * password, so every fill needs a finger and Android never holds the password
     * before the user picks it.
     */
    fun buildResponse(ctx: Context, structure: AssistStructure, inlineRequest: InlineSuggestionsRequest?): FillResponse? {
      val form = Form.of(ctx, structure) ?: return null
      val inline = inlineRequest?.let { Inline(ctx, it) }
      val logins = if (KryptosBridge.ensureReady(ctx)) form.logins() else null
      val response = FillResponse.Builder()

      if (logins == null) {
        // Locked: one entry that asks for the fingerprint and then fills the form directly.
        val title = ctx.getString(R.string.autofill_unlock)
        val ds = form.protectedDataset(ctx, title, form.label, 0, null)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
          @Suppress("DEPRECATION")
          inline?.presentation(0, title, form.label)?.let { ds.setInlinePresentation(it) }
        }
        form.saveInfo()?.let { response.setSaveInfo(it) }
        return response.addDataset(ds.build()).build()
      }

      // Nothing saved for this site yet: no suggestions, but still offer to save.
      if (logins.isEmpty()) return form.saveInfo()?.let { response.setSaveInfo(it).build() }
      val confirm = Biometric.status(ctx) == "on"
      logins.forEachIndexed { i, login ->
        val ds = if (confirm) {
          form.protectedDataset(ctx, login.title, login.subtitle(form), i + 1, login.id)
        } else {
          form.dataset(ctx, login)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
          @Suppress("DEPRECATION")
          inline?.presentation(i, login.title, login.subtitle(form))?.let { ds.setInlinePresentation(it) }
        }
        response.addDataset(ds.build())
      }
      form.saveInfo()?.let { response.setSaveInfo(it) }
      return response.build()
    }

    /**
     * After a fingerprint: the matching logins as ready-to-fill datasets, only the one
     * with [entryId] when given. Matching is redone on the form itself, so the id in
     * the intent cannot pull an entry for another site.
     */
    fun unlockedDatasets(ctx: Context, structure: AssistStructure, entryId: String?): List<Suggestion> {
      val form = Form.of(ctx, structure) ?: return emptyList()
      return form.logins().orEmpty()
        .filter { entryId == null || it.id == entryId }
        .map { Suggestion(it.title, "${it.title} · ${it.subtitle(form)}", form.dataset(ctx, it).build()) }
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

  class Suggestion(val title: String, val label: String, val dataset: Dataset)

  private class Login(val id: String, val title: String, val username: String, val password: String) {
    fun subtitle(form: Form) = username.ifEmpty { form.label }
  }

  /** A login form another app or a browser page is showing. */
  private class Form(val ids: List<AutofillId>, val fields: Fields, val target: String, val label: String) {
    /** Matching logins, or null while the vault is locked. */
    fun logins(): List<Login>? {
      val json = runCatching { KryptosBridge.fillData(target) }.getOrNull() ?: return null
      val arr = JSONArray(json)
      return (0 until arr.length()).map { i ->
        arr.getJSONObject(i).let {
          Login(it.optString("id"), it.optString("title"), it.optString("username"), it.optString("password"))
        }
      }
    }

    /**
     * A suggestion without values: tapping it opens AutofillUnlockActivity, which asks
     * for the fingerprint and returns the filled dataset. The framework adds the form
     * to this intent, hence mutable; it is explicit, so nobody else can receive it.
     * [requestCode] must differ per suggestion, or the intents would replace each other.
     */
    @Suppress("DEPRECATION")
    fun protectedDataset(ctx: Context, title: String, subtitle: String, requestCode: Int, entryId: String?): Dataset.Builder {
      val intent = Intent(ctx, AutofillUnlockActivity::class.java)
      entryId?.let { intent.putExtra(EXTRA_ENTRY_ID, it) }
      val sender = PendingIntent.getActivity(
        ctx, requestCode, intent, PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_CANCEL_CURRENT,
      ).intentSender
      return Dataset.Builder(row(ctx, title, subtitle)).apply {
        ids.forEach { setValue(it, null) }
        setAuthentication(sender)
      }
    }

    /**
     * Asks Android to offer saving once the user has typed a password. Many apps and
     * web pages never finish their activity after logging in, so saving also triggers
     * when the fields disappear.
     */
    fun saveInfo(): SaveInfo? {
      val password = fields.password ?: return null
      val user = fields.username
      val type = SaveInfo.SAVE_DATA_TYPE_PASSWORD or (if (user != null) SaveInfo.SAVE_DATA_TYPE_USERNAME else 0)
      return SaveInfo.Builder(type, arrayOf(password))
        .apply { user?.let { setOptionalIds(arrayOf(it)) } }
        .setFlags(SaveInfo.FLAG_SAVE_ON_ALL_VIEWS_INVISIBLE)
        .build()
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

  /**
   * What the user typed into a login form, collected from every screen of a save
   * request (a username and its password may come from different screens).
   * Uses the same target rules as filling: trusted browsers give the page domain,
   * any other app only its package.
   */
  private class Typed(val target: String, val title: String, val username: String, val password: String) {
    companion object {
      fun of(ctx: Context, structures: List<AssistStructure>): Typed? {
        var form: Form? = null
        var username = ""
        var password = ""
        for (s in structures) {
          val f = Form.of(ctx, s) ?: continue
          form = f
          f.fields.usernameValue?.takeIf { it.isNotEmpty() }?.let { username = it }
          f.fields.passwordValue?.takeIf { it.isNotEmpty() }?.let { password = it }
        }
        val last = form ?: return null
        if (password.isEmpty()) return null
        val title = if (last.target.startsWith("androidapp://")) appLabel(ctx, last.label) else last.label.removePrefix("www.")
        return Typed(last.target, title, username, password)
      }

      private fun appLabel(ctx: Context, pkg: String): String = runCatching {
        val pm = ctx.packageManager
        pm.getApplicationLabel(pm.getApplicationInfo(pkg, 0)).toString()
      }.getOrDefault(pkg)
    }
  }

  /** Finds the username and password fields using autofill hints, input types and HTML attributes. */
  private class Fields {
    var username: AutofillId? = null
    var password: AutofillId? = null
    var webDomain: String? = null
    /** What the fields contain (only read when saving). */
    var usernameValue: String? = null
    var passwordValue: String? = null

    fun scan(s: AssistStructure) {
      for (i in 0 until s.windowNodeCount) visit(s.getWindowNodeAt(i).rootViewNode)
    }

    private fun visit(n: AssistStructure.ViewNode) {
      n.webDomain?.takeIf { it.isNotBlank() && webDomain == null }?.let { webDomain = it }
      val id = n.autofillId
      if (id != null && n.autofillType == View.AUTOFILL_TYPE_TEXT) {
        when {
          password == null && isPassword(n) -> {
            password = id
            passwordValue = text(n)
          }
          username == null && isUsername(n) -> {
            username = id
            usernameValue = text(n)
          }
        }
      }
      for (i in 0 until n.childCount) visit(n.getChildAt(i))
    }

    private fun text(n: AssistStructure.ViewNode): String? =
      n.autofillValue?.takeIf { it.isText }?.textValue?.toString()

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
