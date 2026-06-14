package com.example.messenger

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import android.util.Log
import androidx.activity.enableEdgeToEdge
import org.json.JSONObject
import java.io.File

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // Cold start via "Share": the intent is already on the activity.
    handleShareIntent(intent)
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
