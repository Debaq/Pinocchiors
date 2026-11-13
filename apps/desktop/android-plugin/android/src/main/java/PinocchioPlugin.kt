package com.pinocchio.android

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import androidx.core.content.FileProvider
import app.tauri.Logger
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File

@InvokeArg
class ShareArgs {
    lateinit var path: String
    var mime: String? = null
    var title: String? = null
}

@TauriPlugin
class PinocchioPlugin(private val activity: Activity) : Plugin(activity) {
    /** Menú de compartir de Android con un archivo de la caché de la app */
    @Command
    fun shareFile(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(ShareArgs::class.java)
            val file = File(args.path)
            if (!file.exists()) {
                invoke.reject("No existe ${args.path}")
                return
            }
            val uri = FileProvider.getUriForFile(activity, activity.packageName + ".pinocchio.files", file)
            val send = Intent(Intent.ACTION_SEND).apply {
                type = args.mime ?: "application/octet-stream"
                putExtra(Intent.EXTRA_STREAM, uri)
                // Con ClipData la app que recibe también obtiene el permiso de lectura
                clipData = ClipData.newRawUri(file.name, uri)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            val chooser = Intent.createChooser(send, args.title)
            activity.runOnUiThread {
                try {
                    activity.startActivity(chooser)
                } catch (ex: Exception) {
                    Logger.error(ex.message ?: "shareFile")
                }
            }
            invoke.resolve(JSObject())
        } catch (ex: Exception) {
            val message = ex.message ?: "No se pudo compartir"
            Logger.error(message)
            invoke.reject(message)
        }
    }
}
