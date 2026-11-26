package com.pinocchio.android

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.content.pm.ActivityInfo
import android.net.Uri
import android.os.Build
import android.webkit.WebView
import androidx.core.content.FileProvider
import app.tauri.Logger
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
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
    /** Archivos que otra app mandó a abrir («Abrir con» o «Compartir»), hasta que la app los pida */
    private val opened = mutableListOf<String>()

    override fun load(webView: WebView) {
        super.load(webView)
        // Horizontal siempre (también va en el manifiesto): en vertical la interfaz queda apretada
        activity.runOnUiThread { activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE }
        // La app se abrió con un archivo (si se abrió desde el ícono, no trae ninguno)
        collect(activity.intent)
    }

    /** La app ya estaba abierta y llega otro archivo */
    override fun onNewIntent(intent: Intent) {
        collect(intent)
    }

    private fun collect(intent: Intent?) {
        if (intent == null) return
        val uris = mutableListOf<Uri>()
        when (intent.action) {
            Intent.ACTION_VIEW -> intent.data?.let { uris.add(it) }
            Intent.ACTION_SEND -> stream(intent)?.let { uris.add(it) }
            Intent.ACTION_SEND_MULTIPLE -> uris.addAll(streams(intent))
        }
        // Algunas apps mandan el archivo solo en ClipData
        if (uris.isEmpty() && intent.action != Intent.ACTION_MAIN) {
            val clip = intent.clipData
            if (clip != null) for (i in 0 until clip.itemCount) clip.getItemAt(i).uri?.let { uris.add(it) }
        }
        if (uris.isEmpty()) return
        synchronized(opened) {
            for (uri in uris) {
                val text = uri.toString()
                if (!opened.contains(text)) opened.add(text)
            }
        }
    }

    @Suppress("DEPRECATION")
    private fun stream(intent: Intent): Uri? =
        if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
        else intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM)

    @Suppress("DEPRECATION")
    private fun streams(intent: Intent): List<Uri> =
        (if (Build.VERSION.SDK_INT >= 33) intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java)
        else intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM)) ?: emptyList()

    /** Entrega (y olvida) los archivos que llegaron de otra app */
    @Command
    fun takeOpened(invoke: Invoke) {
        val list = synchronized(opened) {
            val copy = opened.toList()
            opened.clear()
            copy
        }
        val uris = JSArray()
        for (uri in list) uris.put(uri)
        val ret = JSObject()
        ret.put("uris", uris)
        invoke.resolve(ret)
    }

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
