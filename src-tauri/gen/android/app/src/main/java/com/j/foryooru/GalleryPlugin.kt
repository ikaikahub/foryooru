package com.j.foryooru

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.ContentValues
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.webkit.MimeTypeMap
import androidx.activity.result.ActivityResult
import androidx.core.content.FileProvider
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.io.IOException

private const val FOLDER_NAME = "Foryooru"
private const val PICKED_ZIP = "picked.zip"
private val ZIP_TYPES = arrayOf("application/zip", "application/x-zip-compressed", "application/octet-stream")
private const val FOLDER_ACCESS =
  Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION

@InvokeArg
class SaveArgs {
  lateinit var path: String
  lateinit var name: String
  var folder: String? = null
}

@InvokeArg
class CopyArgs {
  lateinit var path: String
}

@TauriPlugin
class GalleryPlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun copyImage(invoke: Invoke) {
    val args = invoke.parseArgs(CopyArgs::class.java)
    val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", File(args.path))
    val clipboard = activity.getSystemService(ClipboardManager::class.java)
    clipboard.setPrimaryClip(ClipData.newUri(activity.contentResolver, "Image", uri))
    invoke.resolve()
  }

  @Command
  fun save(invoke: Invoke) {
    val args = invoke.parseArgs(SaveArgs::class.java)
    val extension = args.name.substringAfterLast('.').lowercase()
    val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension) ?: "image/jpeg"
    val chosen = args.folder
    try {
      val label = if (chosen == null) saveToDownloads(args, mime) else saveToFolder(args, mime, Uri.parse(chosen))
      val result = JSObject()
      result.put("folder", label)
      invoke.resolve(result)
    } catch (error: Exception) {
      invoke.reject(error.message ?: "Couldn't save the file")
    }
  }

  @Command
  fun pickFolder(invoke: Invoke) {
    val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE)
      .addFlags(FOLDER_ACCESS or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
    startActivityForResult(invoke, intent, "folderPicked")
  }

  @ActivityCallback
  fun folderPicked(invoke: Invoke, result: ActivityResult) {
    val tree = result.data?.data
    if (result.resultCode != Activity.RESULT_OK || tree == null) {
      invoke.resolve(JSObject())
      return
    }
    val resolver = activity.contentResolver
    for (granted in resolver.persistedUriPermissions) {
      if (granted.uri != tree) resolver.releasePersistableUriPermission(granted.uri, FOLDER_ACCESS)
    }
    resolver.takePersistableUriPermission(tree, FOLDER_ACCESS)
    val folder = JSObject()
    folder.put("location", tree.toString())
    folder.put("label", labelOf(tree))
    val response = JSObject()
    response.put("folder", folder)
    invoke.resolve(response)
  }

  @Command
  fun pickZip(invoke: Invoke) {
    val intent = Intent(Intent.ACTION_OPEN_DOCUMENT)
      .addCategory(Intent.CATEGORY_OPENABLE)
      .setType("*/*")
      .putExtra(Intent.EXTRA_MIME_TYPES, ZIP_TYPES)
    startActivityForResult(invoke, intent, "zipPicked")
  }

  @ActivityCallback
  fun zipPicked(invoke: Invoke, result: ActivityResult) {
    val picked = result.data?.data
    if (result.resultCode != Activity.RESULT_OK || picked == null) {
      invoke.resolve(JSObject())
      return
    }
    try {
      val copy = File(activity.cacheDir, PICKED_ZIP)
      val input = activity.contentResolver.openInputStream(picked) ?: throw IOException("Android couldn't open the file")
      input.use { source -> copy.outputStream().use { target -> source.copyTo(target) } }
      val response = JSObject()
      response.put("path", copy.absolutePath)
      invoke.resolve(response)
    } catch (error: Exception) {
      invoke.reject(error.message ?: "Couldn't read the file")
    }
  }

  private fun labelOf(tree: Uri): String {
    val path = DocumentsContract.getTreeDocumentId(tree).substringAfter(':')
    return path.ifEmpty { "Storage" }
  }

  private fun saveToFolder(args: SaveArgs, mime: String, tree: Uri): String {
    val resolver = activity.contentResolver
    val parent = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
    val file = DocumentsContract.createDocument(resolver, parent, mime, args.name)
      ?: throw IOException("That folder isn't available any more, pick it again in Settings")
    try {
      val output = resolver.openOutputStream(file) ?: throw IOException("Android couldn't open the file")
      output.use { stream -> File(args.path).inputStream().use { input -> input.copyTo(stream) } }
    } catch (error: Exception) {
      DocumentsContract.deleteDocument(resolver, file)
      throw error
    }
    return labelOf(tree)
  }

  private fun saveToDownloads(args: SaveArgs, mime: String): String {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
      throw IOException("Saving to Downloads needs Android 10 or newer, pick a folder in Settings instead")
    }
    val folder = "${Environment.DIRECTORY_DOWNLOADS}/$FOLDER_NAME"
    val resolver = activity.contentResolver
    val values = ContentValues().apply {
      put(MediaStore.MediaColumns.DISPLAY_NAME, args.name)
      put(MediaStore.MediaColumns.MIME_TYPE, mime)
      put(MediaStore.MediaColumns.RELATIVE_PATH, folder)
      put(MediaStore.MediaColumns.IS_PENDING, 1)
    }
    val uri = resolver.insert(MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY), values)
      ?: throw IOException("Android couldn't create the file")
    try {
      val output = resolver.openOutputStream(uri) ?: throw IOException("Android couldn't open the file")
      output.use { stream -> File(args.path).inputStream().use { input -> input.copyTo(stream) } }
      resolver.update(uri, ContentValues().apply { put(MediaStore.MediaColumns.IS_PENDING, 0) }, null, null)
    } catch (error: Exception) {
      resolver.delete(uri, null, null)
      throw error
    }
    return folder
  }
}
