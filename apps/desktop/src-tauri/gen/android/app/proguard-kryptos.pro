# JNI entry points are looked up by name from Rust.
-keep class com.kryptos.vault.KryptosBridge { native <methods>; *; }

# Called by name from Rust (src-tauri/src/android.rs, with_activity/call_void):
# R8 would otherwise drop them as unused and the calls would silently fail.
-keepclassmembers class com.kryptos.vault.MainActivity {
    public boolean isAutofillEnabled();
    public void openAutofillSettings();
    public java.lang.String biometricStatus();
    public void biometricEnable();
    public void biometricUnlock();
    public void biometricDisable();
    public void onMasterUnlock();
}
