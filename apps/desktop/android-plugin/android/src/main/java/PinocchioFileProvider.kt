package com.pinocchio.android

import androidx.core.content.FileProvider

/**
 * FileProvider propio para compartir archivos de la app. La plantilla de
 * Android de Tauri ya declara `androidx.core.content.FileProvider`; dos
 * proveedores con el mismo nombre de clase rompen la mezcla de manifiestos,
 * así que este es una subclase con nombre propio.
 */
class PinocchioFileProvider : FileProvider()
