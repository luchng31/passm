package com.passm.app

import android.os.Bundle
import android.provider.Settings
import androidx.activity.enableEdgeToEdge
import io.crates.keyring.Keyring

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    enableEdgeToEdge()
    Keyring.initializeNdkContext(applicationContext)
    // Feed the Android system HTTP proxy (set by proxy apps like v2rayNG)
    // into the Rust git transport; libgit2 cannot read it itself.
    val host = Settings.Global.getString(contentResolver, Settings.Global.HTTP_PROXY)
    Keyring.setHttpProxy(host)
  }
}
