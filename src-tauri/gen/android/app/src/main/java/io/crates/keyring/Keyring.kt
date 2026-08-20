package io.crates.keyring

import android.content.Context

class Keyring {
    companion object {
        init {
            // Load the app's main native library: it statically links the
            // android-native-keyring-store rlib, so this JNI symbol shares the
            // same ndk-context global that Store::new() reads. Loading the
            // standalone cdylib instead would initialize a *different*
            // ndk-context copy, leaving the main lib's context uninitialized
            // and causing "android context was not initialized" panics.
            System.loadLibrary("passm_app_lib")
        }

        external fun initializeNdkContext(context: Context)
        external fun setHttpProxy(proxy: String?)
    }
}