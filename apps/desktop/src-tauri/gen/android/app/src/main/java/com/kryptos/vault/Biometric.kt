package com.kryptos.vault

import android.content.Context
import android.os.Build
import android.provider.Settings
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Fingerprint unlock.
 *
 * The vault key is encrypted with an AES key that lives in the Android Keystore
 * (secure hardware) and can only be used right after a *strong* biometric check,
 * for that single operation. Enrolling a new fingerprint destroys that key, so
 * someone who learns the phone PIN cannot add their own finger to get in.
 * Only the encrypted vault key is stored (app-private prefs, excluded from backups).
 *
 * The master password stays mandatory after every reboot and every 7 days, so it
 * is not forgotten and a long-running unlocked phone has a bounded window.
 *
 * Results are reported as codes: "ok", "cancelled", "use_master", "lockout",
 * "invalidated", "master_required", "locked", "off", "failed".
 */
object Biometric {
  private const val KEY_ALIAS = "kryptos_vault_key"
  private const val PREFS = "kryptos_biometric"
  private const val TRANSFORM = "AES/GCM/NoPadding"
  private const val MAX_AGE_MS = 7L * 24 * 60 * 60 * 1000
  private const val AUTH = BiometricManager.Authenticators.BIOMETRIC_STRONG

  /** "unavailable" | "off" | "on" | "master_required" */
  fun status(ctx: Context): String = when {
    BiometricManager.from(ctx).canAuthenticate(AUTH) != BiometricManager.BIOMETRIC_SUCCESS -> "unavailable"
    !isEnabled(ctx) -> "off"
    masterRequired(ctx) -> "master_required"
    else -> "on"
  }

  /** Call whenever the master password has just been verified. */
  fun onMasterUnlock(ctx: Context) {
    prefs(ctx).edit().putLong("verified_at", System.currentTimeMillis()).putInt("boot", bootCount(ctx)).apply()
  }

  /** Seals the (currently unlocked) vault key under a new fingerprint-bound key. */
  fun enable(activity: FragmentActivity, done: (String) -> Unit) {
    val vaultKey = KryptosBridge.vaultKey() ?: return done("locked")
    val cipher = runCatching { Cipher.getInstance(TRANSFORM).apply { init(Cipher.ENCRYPT_MODE, createKey()) } }
      .getOrElse { vaultKey.fill(0); return done("failed") }
    prompt(activity, cipher, R.string.biometric_enable_title) { c, error ->
      val sealed = c?.let { runCatching { it.doFinal(vaultKey) }.getOrNull() }
      vaultKey.fill(0)
      if (c == null || sealed == null) return@prompt done(if (c == null) error else "failed")
      prefs(activity).edit().putString("iv", b64(c.iv)).putString("ct", b64(sealed)).apply()
      done("ok")
    }
  }

  /** Asks for the fingerprint and unlocks the vault in Rust with the released key. */
  fun unlock(activity: FragmentActivity, done: (String) -> Unit) {
    when (status(activity)) {
      "on" -> {}
      "master_required" -> return done("master_required")
      else -> return done("off")
    }
    val p = prefs(activity)
    val iv = unb64(p.getString("iv", null))
    val sealed = unb64(p.getString("ct", null))
    val cipher = runCatching {
      Cipher.getInstance(TRANSFORM).apply { init(Cipher.DECRYPT_MODE, loadKey(), GCMParameterSpec(128, iv)) }
    }.getOrElse { e ->
      // New fingerprint enrolled (or the key is gone): start over with the master password.
      disable(activity)
      return done(if (e is KeyPermanentlyInvalidatedException) "invalidated" else "failed")
    }
    prompt(activity, cipher, R.string.biometric_unlock_title) { c, error ->
      if (c == null) return@prompt done(error)
      val vaultKey = runCatching { c.doFinal(sealed) }.getOrNull() ?: return@prompt done("failed")
      val ok = KryptosBridge.unlockWithKey(vaultKey)
      vaultKey.fill(0)
      if (!ok) disable(activity) // the vault was replaced: this key no longer opens it
      done(if (ok) "ok" else "invalidated")
    }
  }

  fun disable(ctx: Context) {
    prefs(ctx).edit().remove("iv").remove("ct").apply()
    runCatching { keyStore().deleteEntry(KEY_ALIAS) }
  }

  private fun isEnabled(ctx: Context) = prefs(ctx).contains("ct")

  private fun masterRequired(ctx: Context): Boolean {
    val p = prefs(ctx)
    val age = System.currentTimeMillis() - p.getLong("verified_at", 0)
    return age !in 0..MAX_AGE_MS || p.getInt("boot", -1) != bootCount(ctx)
  }

  private fun bootCount(ctx: Context) = Settings.Global.getInt(ctx.contentResolver, Settings.Global.BOOT_COUNT, 0)

  private fun prompt(activity: FragmentActivity, cipher: Cipher, title: Int, done: (Cipher?, String) -> Unit) {
    val info = BiometricPrompt.PromptInfo.Builder()
      .setTitle(activity.getString(title))
      .setSubtitle(activity.getString(R.string.biometric_subtitle))
      .setNegativeButtonText(activity.getString(R.string.biometric_use_master))
      .setAllowedAuthenticators(AUTH)
      .setConfirmationRequired(false)
      .build()
    val callback = object : BiometricPrompt.AuthenticationCallback() {
      override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) =
        done(result.cryptoObject?.cipher, "failed")

      override fun onAuthenticationError(code: Int, message: CharSequence) = done(
        null,
        when (code) {
          BiometricPrompt.ERROR_NEGATIVE_BUTTON -> "use_master"
          BiometricPrompt.ERROR_USER_CANCELED, BiometricPrompt.ERROR_CANCELED -> "cancelled"
          BiometricPrompt.ERROR_LOCKOUT, BiometricPrompt.ERROR_LOCKOUT_PERMANENT -> "lockout"
          else -> "failed"
        },
      )
      // onAuthenticationFailed (finger not recognised): the prompt stays open for another try.
    }
    BiometricPrompt(activity, ContextCompat.getMainExecutor(activity), callback)
      .authenticate(info, BiometricPrompt.CryptoObject(cipher))
  }

  private fun createKey(): SecretKey {
    runCatching { keyStore().deleteEntry(KEY_ALIAS) }
    val spec = KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
      .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
      .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
      .setKeySize(256)
      .setUserAuthenticationRequired(true)
      .setInvalidatedByBiometricEnrollment(true)
      .apply {
        // Every use needs its own fingerprint (no "valid for N seconds" window).
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
          setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
        } else {
          @Suppress("DEPRECATION")
          setUserAuthenticationValidityDurationSeconds(-1)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) setUnlockedDeviceRequired(true)
      }
      .build()
    return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
      init(spec)
      generateKey()
    }
  }

  private fun loadKey() = keyStore().getKey(KEY_ALIAS, null) as SecretKey

  private fun keyStore() = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

  private fun prefs(ctx: Context) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

  private fun b64(b: ByteArray) = Base64.encodeToString(b, Base64.NO_WRAP)

  private fun unb64(s: String?) = Base64.decode(s ?: "", Base64.NO_WRAP)
}
