package com.kryptos.vault

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.DocumentsContract.Document

/**
 * The cloud folder Kryptos syncs through (see cloud.rs), as a folder the user
 * picked once with the system picker (Google Drive, Dropbox... show up there).
 * Kryptos keeps no INTERNET permission: the provider's app moves the file, and
 * this only reads and writes `<folder>/Kryptos/vault.kryptos` through Android's
 * document APIs. Called from Rust on a background thread.
 */
object CloudFolder {
  private const val DIR = "Kryptos"
  private const val FILE = "vault.kryptos"
  private lateinit var app: Context

  fun init(context: Context) {
    app = context.applicationContext
  }

  /** Whether Kryptos may still use [tree] (the permission can be revoked in settings). */
  @JvmStatic
  fun usable(tree: String): Boolean = runCatching {
    val uri = Uri.parse(tree)
    app.contentResolver.persistedUriPermissions.any { it.uri == uri && it.isReadPermission && it.isWritePermission }
  }.getOrDefault(false)

  /** The picked folder's name, for the UI ("My Drive", "Dropbox"...). */
  @JvmStatic
  fun name(tree: String): String = runCatching {
    val uri = Uri.parse(tree)
    val doc = DocumentsContract.buildDocumentUriUsingTree(uri, DocumentsContract.getTreeDocumentId(uri))
    query(doc, Document.COLUMN_DISPLAY_NAME) { it.getString(0) }
  }.getOrNull().orEmpty()

  /** Contents of the vault copy, or null if there is none yet. */
  @JvmStatic
  fun read(tree: String): ByteArray? = runCatching { find(tree, FILE)?.let(::readDoc) }.getOrNull()

  /** Replaces (or creates) the vault copy. */
  @JvmStatic
  fun write(tree: String, bytes: ByteArray): Boolean = writeNamed(tree, FILE, bytes)

  /** A team's sealed key file next to the vault copy, or null if there is none. */
  @JvmStatic
  fun readNamed(tree: String, name: String): ByteArray? = runCatching { find(tree, name)?.let(::readDoc) }.getOrNull()

  @JvmStatic
  fun writeNamed(tree: String, name: String, bytes: ByteArray): Boolean = runCatching {
    val dir = dir(tree, create = true) ?: return false
    val doc = child(dir, name) ?: DocumentsContract.createDocument(app.contentResolver, dir, "application/octet-stream", name)
      ?: return false
    app.contentResolver.openOutputStream(doc, "wt")?.use { it.write(bytes) } ?: return false
    true
  }.getOrDefault(false)

  @JvmStatic
  fun deleteNamed(tree: String, name: String): Boolean = runCatching {
    find(tree, name)?.let { DocumentsContract.deleteDocument(app.contentResolver, it) } ?: false
  }.getOrDefault(false)

  /** Last-modified time of the vault copy in ms, or -1 if there is none. */
  @JvmStatic
  fun modified(tree: String): Long = runCatching {
    find(tree, FILE)?.let { query(it, Document.COLUMN_LAST_MODIFIED) { c -> c.getLong(0) } } ?: -1L
  }.getOrDefault(-1L)

  /** Copies the provider made when two devices wrote at once ("vault (1).kryptos"...), as document URIs. */
  @JvmStatic
  fun conflicts(tree: String): Array<String> = runCatching {
    val dir = dir(tree, create = false) ?: return emptyArray()
    children(dir).filter { (name, _) -> name != FILE && name.startsWith("vault") && name.endsWith(".kryptos") }
      .map { it.second.toString() }.toTypedArray()
  }.getOrDefault(emptyArray())

  @JvmStatic
  fun readDoc(doc: String): ByteArray? = runCatching { readDoc(Uri.parse(doc)) }.getOrNull()

  @JvmStatic
  fun delete(doc: String): Boolean = runCatching { DocumentsContract.deleteDocument(app.contentResolver, Uri.parse(doc)) }.getOrDefault(false)

  private fun readDoc(doc: Uri): ByteArray? = app.contentResolver.openInputStream(doc)?.use { it.readBytes() }

  private fun find(tree: String, name: String): Uri? = dir(tree, create = false)?.let { child(it, name) }

  /**
   * `<picked folder>/Kryptos`, created on first write. If the user picked the
   * Kryptos folder itself (made by another device), that folder is used as is:
   * nesting a second one there would leave each device with its own vault.
   */
  private fun dir(tree: String, create: Boolean): Uri? {
    val treeUri = Uri.parse(tree)
    val root = DocumentsContract.buildDocumentUriUsingTree(treeUri, DocumentsContract.getTreeDocumentId(treeUri))
    val names = children(root).map { it.first }
    if (FILE in names || (DIR !in names && query(root, Document.COLUMN_DISPLAY_NAME) { it.getString(0) } == DIR)) return root
    return child(root, DIR) ?: if (create) DocumentsContract.createDocument(app.contentResolver, root, Document.MIME_TYPE_DIR, DIR) else null
  }

  private fun child(parent: Uri, name: String): Uri? = children(parent).firstOrNull { it.first == name }?.second

  private fun children(parent: Uri): List<Pair<String, Uri>> {
    val children = DocumentsContract.buildChildDocumentsUriUsingTree(parent, DocumentsContract.getDocumentId(parent))
    val out = mutableListOf<Pair<String, Uri>>()
    app.contentResolver.query(children, arrayOf(Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME), null, null, null)?.use { c ->
      while (c.moveToNext()) {
        out += c.getString(1) to DocumentsContract.buildDocumentUriUsingTree(parent, c.getString(0))
      }
    }
    return out
  }

  private fun <T> query(doc: Uri, column: String, read: (android.database.Cursor) -> T): T? =
    app.contentResolver.query(doc, arrayOf(column), null, null, null)?.use { c -> if (c.moveToFirst()) read(c) else null }
}
