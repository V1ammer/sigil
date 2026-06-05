package com.example.keystore

import android.app.Activity
import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

/**
 * Tauri plugin providing secure credential storage on Android
 * using EncryptedSharedPreferences backed by MasterKey (AES-256 GCM).
 */
@TauriPlugin
class KeystorePlugin(activity: Activity) : Plugin(activity) {

    private val prefs: SharedPreferences by lazy {
        val masterKey = MasterKey.Builder(activity.applicationContext)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build()

        EncryptedSharedPreferences.create(
            activity.applicationContext,
            PREF_NAME,
            masterKey,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM
        )
    }

    @Command
    fun set(invoke: Invoke) {
        try {
            val args = invoke.getArgs()
            val key = args.getString("key")!!
            val value = args.getString("value")!!

            if (key.isEmpty()) return invoke.reject("key is required")
            if (value.isEmpty()) return invoke.reject("value is required")

            prefs.edit().putString(key, value).apply()
            invoke.resolve()
        } catch (ex: Exception) {
            invoke.reject(ex.message)
        }
    }

    @Command
    fun get(invoke: Invoke) {
        try {
            val args = invoke.getArgs()
            val key = args.getString("key")!!

            if (key.isEmpty()) return invoke.reject("key is required")

            val value = prefs.getString(key, null)
            val result = JSObject()
            result.put("value", value) // null will be stored as JSON null
            invoke.resolve(result)
        } catch (ex: Exception) {
            invoke.reject(ex.message)
        }
    }

    @Command
    fun delete(invoke: Invoke) {
        try {
            val args = invoke.getArgs()
            val key = args.getString("key")!!

            if (key.isEmpty()) return invoke.reject("key is required")

            prefs.edit().remove(key).apply()
            invoke.resolve()
        } catch (ex: Exception) {
            invoke.reject(ex.message)
        }
    }

    companion object {
        private const val PREF_NAME = "com.example.keystore.encrypted_prefs"
    }
}
