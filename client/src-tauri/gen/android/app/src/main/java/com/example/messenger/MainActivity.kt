package com.example.messenger

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.OpenableColumns
import android.util.Log
import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import android.widget.Toast
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import org.json.JSONObject
import java.io.File

class MainActivity : TauriActivity() {
  private var backPressedOnce = false

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // TEMP DIAGNOSTIC: remote debugging for chrome://inspect while we debug video send.
    WebView.setWebContentsDebuggingEnabled(true)
    // Cold start via "Share": the intent is already on the activity.
    handleShareIntent(intent)

    // Route the hardware back button through the WebView's overlay stack: a
    // back press first closes the open chat/thread/dialog (window.__androidBack);
    // only at the chat list (root) does it fall through to "press again to exit"
    // — instead of Tauri's default, which exits the app from anywhere.
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        val wv = findWebView(window.decorView)
        if (wv == null) {
          confirmExit()
          return
        }
        wv.evaluateJavascript("(window.__androidBack && window.__androidBack()) ? 'true' : 'false'") { result ->
          if (result == null || !result.contains("true")) {
            confirmExit()
          }
          // else: an overlay was closed in the WebView — consume the back.
        }
      }
    })
  }

  /** Two-step exit: first back shows a hint, a second back within 2s exits. */
  private fun confirmExit() {
    if (backPressedOnce) {
      finish()
      return
    }
    backPressedOnce = true
    Toast.makeText(applicationContext, "Нажмите «Назад» ещё раз для выхода", Toast.LENGTH_SHORT).show()
    Handler(Looper.getMainLooper()).postDelayed({ backPressedOnce = false }, 2000)
  }

  /** Find the Tauri/wry WebView in the activity's view tree. */
  private fun findWebView(view: View): WebView? {
    if (view is WebView) return view
    if (view is ViewGroup) {
      for (i in 0 until view.childCount) {
        findWebView(view.getChildAt(i))?.let { return it }
      }
    }
    return null
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    // Warm start (app already running, launchMode=singleTask): a fresh share
    // arrives here. The WebView reads it on its next focus poll.
    setIntent(intent)
    handleShareIntent(intent)
  }

  /**
   * If this intent is a "Share" of one or more files, copy each into a private
   * inbox dir (filesDir/share_inbox) as a <id>.data blob + <id>.json metadata.
   * The WASM frontend polls `take_shared_attachments` (Rust) which reads and
   * clears that dir, then stages the items into the composer.
   */
  private fun handleShareIntent(intent: Intent?) {
    if (intent == null) return
    val uris: List<Uri> = when (intent.action) {
      Intent.ACTION_SEND -> {
        val u = if (android.os.Build.VERSION.SDK_INT >= 33)
          intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
        else
          @Suppress("DEPRECATION") intent.getParcelableExtra(Intent.EXTRA_STREAM)
        if (u != null) listOf(u) else emptyList()
      }
      Intent.ACTION_SEND_MULTIPLE -> {
        val list = if (android.os.Build.VERSION.SDK_INT >= 33)
          intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java)
        else
          @Suppress("DEPRECATION") intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM)
        list ?: emptyList()
      }
      else -> emptyList()
    }
    if (uris.isEmpty()) return

    val inbox = File(filesDir, "share_inbox")
    if (!inbox.exists()) inbox.mkdirs()
    Log.d("ShareInbox", "share: ${uris.size} uri(s), inbox=${inbox.absolutePath}")

    for ((i, uri) in uris.withIndex()) {
      try {
        val resolver = contentResolver
        val mime = resolver.getType(uri) ?: intent.type ?: "application/octet-stream"
        val name = queryDisplayName(uri) ?: "shared-${System.currentTimeMillis()}-$i"
        val id = "${System.currentTimeMillis()}-$i"
        val dataFile = File(inbox, "$id.data")
        resolver.openInputStream(uri).use { input ->
          if (input == null) return@use
          dataFile.outputStream().use { out -> input.copyTo(out) }
        }
        if (dataFile.exists() && dataFile.length() > 0) {
          val meta = JSONObject()
          meta.put("name", name)
          meta.put("mime", mime)
          File(inbox, "$id.json").writeText(meta.toString())
          Log.d("ShareInbox", "wrote $id ($mime, ${dataFile.length()} bytes) name=$name")
        }
      } catch (e: Exception) {
        // Skip an unreadable share item; the others still go through.
        Log.w("ShareInbox", "failed for $uri: ${e.message}")
      }
    }
  }

  private fun queryDisplayName(uri: Uri): String? {
    return try {
      contentResolver.query(uri, null, null, null, null)?.use { c ->
        val idx = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
        if (idx >= 0 && c.moveToFirst()) c.getString(idx) else null
      }
    } catch (_: Exception) {
      null
    }
  }
}
