package com.j.foryooru

import android.graphics.Rect
import android.os.Build
import android.os.Bundle
import android.view.View
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

private const val MENU_EDGE_DP = 32
private const val MENU_BAND_DP = 200
private const val MENU_BAND_TOP_SHARE = 0.35

class MainActivity : TauriActivity() {
  private var webView: WebView? = null

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    webView.settings.mediaPlaybackRequiresUserGesture = false
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      webView.addOnLayoutChangeListener { view, _, _, _, _, _, _, _, _ ->
        val density = resources.displayMetrics.density
        val top = (view.height * MENU_BAND_TOP_SHARE).toInt()
        val band = Rect(0, top, (MENU_EDGE_DP * density).toInt(), top + (MENU_BAND_DP * density).toInt())
        view.systemGestureExclusionRects = listOf(band)
      }
    }
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    ViewCompat.setOnApplyWindowInsetsListener(findViewById<View>(android.R.id.content)) { view, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
      val keyboard = (insets.getInsets(WindowInsetsCompat.Type.ime()).bottom - bars.bottom).coerceAtLeast(0)
      val keyboardCss = keyboard / resources.displayMetrics.density
      webView?.evaluateJavascript("dispatchEvent(new CustomEvent('keyboardinset', { detail: $keyboardCss }))", null)
      WindowInsetsCompat.CONSUMED
    }
  }
}
