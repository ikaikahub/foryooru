package com.j.foryooru

import android.annotation.SuppressLint
import android.app.Activity
import android.app.Dialog
import android.view.ViewGroup
import android.webkit.CookieManager
import android.webkit.WebView
import android.webkit.WebViewClient
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

private const val SITE_URL = "https://gelbooru.com"
private const val LOGIN_URL = "https://gelbooru.com/index.php?page=account&s=login&code=00"

@TauriPlugin
class GelbooruLoginPlugin(private val activity: Activity) : Plugin(activity) {
  @SuppressLint("SetJavaScriptEnabled")
  @Command
  fun login(invoke: Invoke) {
    activity.runOnUiThread {
      val webView = WebView(activity)
      webView.settings.javaScriptEnabled = true
      webView.settings.domStorageEnabled = true
      val dialog = Dialog(activity, android.R.style.Theme_DeviceDefault_NoActionBar)
      var finished = false

      fun finish(session: JSObject?) {
        if (finished) return
        finished = true
        val answer = JSObject()
        if (session != null) answer.put("session", session)
        invoke.resolve(answer)
        dialog.dismiss()
        webView.destroy()
      }

      webView.webViewClient = object : WebViewClient() {
        override fun onPageFinished(view: WebView, url: String) {
          val session = sessionFrom(CookieManager.getInstance().getCookie(SITE_URL))
          if (session != null) finish(session)
        }
      }
      dialog.setContentView(
        webView,
        ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT),
      )
      dialog.setOnDismissListener { finish(null) }
      dialog.show()
      webView.loadUrl(LOGIN_URL)
    }
  }

  @Command
  fun logout(invoke: Invoke) {
    activity.runOnUiThread {
      val manager = CookieManager.getInstance()
      for (name in cookiesFrom(manager.getCookie(SITE_URL)).keys) {
        manager.setCookie(SITE_URL, "$name=; Max-Age=0; Path=/")
        manager.setCookie(SITE_URL, "$name=; Max-Age=0; Path=/; Domain=.gelbooru.com")
      }
      manager.flush()
      invoke.resolve()
    }
  }

  private fun cookiesFrom(header: String?): Map<String, String> =
    header.orEmpty()
      .split(';')
      .map { it.trim().split('=', limit = 2) }
      .filter { it.size == 2 }
      .associate { it[0] to it[1] }

  private fun sessionFrom(header: String?): JSObject? {
    val cookies = cookiesFrom(header)
    val userId = cookies["user_id"]?.takeIf { it.isNotEmpty() } ?: return null
    val passHash = cookies["pass_hash"]?.takeIf { it.isNotEmpty() } ?: return null
    val session = JSObject()
    session.put("userId", userId)
    session.put("passHash", passHash)
    return session
  }
}
