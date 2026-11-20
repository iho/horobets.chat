package messenger.app

import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import java.io.File
import java.util.UUID
import java.util.concurrent.Executors

/** Native document access only; networking and persistent caching stay in Rust. */
object MobileFiles {
    private var activity: ComponentActivity? = null
    private var picker: ActivityResultLauncher<String>? = null
    private var saver: ActivityResultLauncher<String>? = null
    private var photos: ActivityResultLauncher<PickVisualMediaRequest>? = null
    private var picked: ((String?, String?) -> Unit)? = null
    private var saved: ((String?) -> Unit)? = null
    private var source: String? = null
    private var limit = 10L * 1024 * 1024
    private val io = Executors.newSingleThreadExecutor()

    fun install(owner: ComponentActivity) {
        activity = owner
        picker = owner.registerForActivityResult(ActivityResultContracts.GetContent()) { uri -> importFile(owner, uri) }
        saver = owner.registerForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
            val completion = saved
            val path = source
            saved = null; source = null
            if (uri == null || path == null) { completion?.invoke(null); return@registerForActivityResult }
            io.execute {
                try {
                    owner.contentResolver.openOutputStream(uri).use { output ->
                        requireNotNull(output) { "Cannot save file" }
                        File(path).inputStream().use { it.copyTo(output) }
                    }
                    owner.runOnUiThread { completion?.invoke(null) }
                } catch (error: Exception) {
                    owner.runOnUiThread { completion?.invoke(error.message ?: "Cannot save file") }
                }
            }
        }
        photos = owner.registerForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri -> importFile(owner, uri) }
    }
    private fun importFile(owner: ComponentActivity, uri: Uri?) {
            val completion = picked
            picked = null
            if (uri == null) { completion?.invoke(null, null); return }
            val maxSize = limit
            io.execute {
                try {
                    var name = "attachment"
                    owner.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
                        if (it.moveToFirst()) name = it.getString(0) ?: name
                    }
                    name = name.substringAfterLast('/').substringAfterLast('\\')
                    if (name.isBlank() || name == "." || name == "..") name = "attachment"
                    val folder = File(owner.cacheDir, "imports/${UUID.randomUUID()}").apply { mkdirs() }
                    val output = File(folder, name)
                    try {
                        owner.contentResolver.openInputStream(uri).use { input ->
                            requireNotNull(input) { "Cannot open selected file" }
                            output.outputStream().use { target ->
                                val buffer = ByteArray(65536)
                                var total = 0L
                                while (true) {
                                    val count = input.read(buffer)
                                    if (count < 0) break
                                    total += count
                                    require(total <= maxSize) { "File is too large (${maxSize / 1024 / 1024} MiB maximum)" }
                                    target.write(buffer, 0, count)
                                }
                            }
                        }
                    } catch (error: Exception) { folder.deleteRecursively(); throw error }
                    owner.runOnUiThread { completion?.invoke(output.absolutePath, null) }
                } catch (error: Exception) {
                    owner.runOnUiThread { completion?.invoke(null, error.message ?: "Cannot import file") }
                }
            }
    }
    fun pickFile(imagesOnly: Boolean, photoLibrary: Boolean, avatar: Boolean, completion: (String?, String?) -> Unit) {
        val launch = picker
        if (launch == null) { completion(null, "File picker is unavailable"); return }
        if (picked != null) { completion(null, "Finish selecting the previous file first"); return }
        limit = (if (avatar) 5L else 10L) * 1024 * 1024
        picked = completion
        if (photoLibrary) {
            val photoLaunch = photos
            if (photoLaunch == null) { picked = null; completion(null, "Photo picker is unavailable"); return }
            photoLaunch.launch(PickVisualMediaRequest.Builder().setMediaType(ActivityResultContracts.PickVisualMedia.ImageOnly).build())
        } else { launch.launch(if (imagesOnly) "image/*" else "*/*") }
    }
    fun copyText(text: String) {
        val owner = activity ?: return
        val clipboard = owner.getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
        clipboard.setPrimaryClip(android.content.ClipData.newPlainText("Message", text))
    }
    fun saveFile(path: String, completion: (String?) -> Unit) {
        val launch = saver
        if (launch == null) { completion("File export is unavailable"); return }
        if (saved != null) { completion("Finish saving the previous file first"); return }
        source = path; saved = completion
        launch.launch(File(path).name)
    }
}
