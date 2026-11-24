package com.koala.music

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import uniffi.koala.audioDuration
import uniffi.koala.decryptNcmFile
import uniffi.koala.scanNcmFolder
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.joinAll
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import kotlinx.coroutines.withContext
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

// 解密状态：0 等待 / 1 进行中 / 2 成功 / 3 失败
private const val STATUS_PENDING = 0
private const val STATUS_RUNNING = 1
private const val STATUS_DONE = 2
private const val STATUS_FAILED = 3

data class UiRecord(
    val sourcePath: String,
    val sourceName: String,
    val outputName: String,
    val fileSize: String,
    val status: Int,
    val musicLength: String,
    val decryptDate: String,
    val durationText: String,
    val errorMessage: String,
)

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                Surface(Modifier.fillMaxSize()) {
                    KoalaScreen()
                }
            }
        }
    }
}

@Composable
private fun KoalaScreen() {
    var sourcePath by remember { mutableStateOf("") }
    var targetPath by remember { mutableStateOf("") }
    var records by remember { mutableStateOf(listOf<UiRecord>()) }
    var running by remember { mutableStateOf(false) }
    var message by remember { mutableStateOf("") }
    val scope = rememberCoroutineScope()

    fun updateAt(index: Int, transform: (UiRecord) -> UiRecord) {
        records = records.toMutableList().also {
            if (index < it.size) it[index] = transform(it[index])
        }
    }

    fun ensureStoragePermission(): Boolean {
        if (Environment.isExternalStorageManager()) return true
        try {
            startActivity(
                Intent(
                    Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                    Uri.parse("package:$packageName"),
                )
            )
        } catch (e: Exception) {
            try {
                startActivity(Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION))
            } catch (_: Exception) {
            }
        }
        message = "请先在系统设置中授予\"所有文件访问\"权限"
        return false
    }

    val sourcePicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        val path = uri?.let { treeUriToPath(it) }
        if (path == null) {
            message = "该目录无法映射到本地路径，请选择内部存储中的目录"
        } else {
            sourcePath = path
            if (targetPath.isNotBlank()) scope.launch {
                try { records = rescanInto(path, targetPath) } catch (e: Exception) {
                    message = "扫描失败: ${e.message}"
                }
            }
        }
    }
    val targetPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        val path = uri?.let { treeUriToPath(it) }
        if (path == null) {
            message = "该目录无法映射到本地路径，请选择内部存储中的目录"
        } else {
            targetPath = path
            if (sourcePath.isNotBlank()) scope.launch {
                try { records = rescanInto(sourcePath, path) } catch (e: Exception) {
                    message = "扫描失败: ${e.message}"
                }
            }
        }
    }

    fun rescan(source: String, target: String) {
        if (!ensureStoragePermission() || source.isBlank()) return
        scope.launch {
            running = true
            try {
                records = rescanInto(source, target)
                message = if (records.isEmpty()) "没有找到需要解密的文件" else ""
            } catch (e: Exception) {
                message = "扫描失败: ${e.message}"
            } finally {
                running = false
            }
        }
    }

    fun decryptOne(index: Int) {
        scope.launch(Dispatchers.IO) {
            updateAt(index) { it.copy(status = STATUS_RUNNING, errorMessage = "") }
            val rec = records[index]
            val outcome = decryptNcmFile(rec.sourcePath, targetPath)
            val newRecord = rec.copy(
                status = if (outcome.success) STATUS_DONE else STATUS_FAILED,
                durationText = outcome.durationText,
                musicLength = if (outcome.success) {
                    audioDuration("$targetPath/${outcome.outputName}")
                } else rec.musicLength,
                decryptDate = if (outcome.success) nowText() else rec.decryptDate,
                errorMessage = outcome.errorMessage,
            )
            updateAt(index) { newRecord }
        }
    }

    fun startDecrypt() {
        val source = sourcePath
        val target = targetPath
        if (source.isBlank() || target.isBlank() || running) return
        if (!ensureStoragePermission()) return
        scope.launch {
            running = true
            try {
                val semaphore = Semaphore(4)
                val jobs = records.mapIndexed { i, rec ->
                    if (rec.status == STATUS_DONE) return@mapIndexed null
                    launch(Dispatchers.IO) {
                        semaphore.withPermit {
                            updateAt(i) { it.copy(status = STATUS_RUNNING, errorMessage = "") }
                            val outcome = decryptNcmFile(rec.sourcePath, target)
                            val newRecord = rec.copy(
                                status = if (outcome.success) STATUS_DONE else STATUS_FAILED,
                                durationText = outcome.durationText,
                                musicLength = if (outcome.success) {
                                    audioDuration("$target/${outcome.outputName}")
                                } else rec.musicLength,
                                decryptDate = if (outcome.success) nowText() else rec.decryptDate,
                                errorMessage = outcome.errorMessage,
                            )
                            updateAt(i) { newRecord }
                        }
                    }
                }.filterNotNull()
                jobs.joinAll()
            } finally {
                running = false
            }
        }
    }

    Column(Modifier.fillMaxSize().padding(16.dp)) {
        Text("Koala", fontSize = 22.sp, fontWeight = FontWeight.Bold)
        Spacer(Modifier.height(12.dp))

        PathField(label = "源目录（ncm 文件）", value = sourcePath) {
            if (ensureStoragePermission()) sourcePicker.launch(null)
        }
        Spacer(Modifier.height(8.dp))
        PathField(label = "输出目录", value = targetPath) {
            if (ensureStoragePermission()) targetPicker.launch(null)
        }
        Spacer(Modifier.height(12.dp))

        Row {
            Button(
                onClick = { startDecrypt() },
                enabled = !running && sourcePath.isNotBlank() && targetPath.isNotBlank(),
            ) { Text(if (running) "处理中..." else "开始解密") }
            Spacer(Modifier.width(12.dp))
            OutlinedButton(
                onClick = { rescan(sourcePath, targetPath) },
                enabled = !running && sourcePath.isNotBlank(),
            ) { Text("刷新列表") }
        }

        if (message.isNotBlank()) {
            Spacer(Modifier.height(8.dp))
            Text(message, color = MaterialTheme.colorScheme.error, fontSize = 13.sp)
        }

        Spacer(Modifier.height(12.dp))
        Text("输出记录（${records.size}）", fontSize = 15.sp, fontWeight = FontWeight.Medium)
        Spacer(Modifier.height(6.dp))

        LazyColumn(
            Modifier.fillMaxSize(),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            itemsIndexed(records) { index, rec ->
                RecordRow(rec) { decryptOne(index) }
            }
        }
    }
}

private fun nowText(): String =
    SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.getDefault()).format(Date())

/** 在 IO 线程扫描目录并返回 UI 记录列表（onDone 供调用方额外处理） */
private suspend fun rescanInto(source: String, target: String): List<UiRecord> =
    withContext(Dispatchers.IO) {
        scanNcmFolder(source, target).map {
            UiRecord(
                sourcePath = it.sourcePath,
                sourceName = it.sourceName,
                outputName = it.outputName,
                fileSize = it.fileSize,
                status = if (it.alreadyExists) STATUS_DONE else STATUS_PENDING,
                musicLength = it.musicLength,
                decryptDate = it.decryptDate,
                durationText = if (it.alreadyExists) "已跳过" else "-",
                errorMessage = "",
            )
        }
    }

@Composable
private fun PathField(label: String, value: String, onBrowse: () -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        OutlinedTextField(
            value = value,
            onValueChange = { /* 由目录选择器写入 */ },
            modifier = Modifier.weight(1f),
            label = { Text(label) },
            singleLine = true,
            readOnly = true,
        )
        Spacer(Modifier.width(8.dp))
        OutlinedButton(onClick = onBrowse) { Text("浏览") }
    }
}

@Composable
private fun RecordRow(rec: UiRecord, onRetry: () -> Unit) {
    val borderColor = when (rec.status) {
        STATUS_DONE -> Color(0xFF66BB6A)
        STATUS_FAILED -> Color(0xFFEF5350)
        else -> Color(0xFFE0E0E0)
    }
    Column(
        Modifier
            .fillMaxWidth()
            .background(Color(0xFFFAFAFA), RoundedCornerShape(6.dp))
            .border(1.dp, borderColor, RoundedCornerShape(6.dp))
            .padding(12.dp),
    ) {
        Text(
            rec.sourceName,
            fontSize = 14.sp,
            fontWeight = FontWeight.Medium,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        Spacer(Modifier.height(4.dp))
        Text(
            "→ ${rec.outputName}",
            fontSize = 12.sp,
            color = Color(0xFF888888),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Spacer(Modifier.height(4.dp))
        val statusText = when (rec.status) {
            STATUS_PENDING -> "等待处理"
            STATUS_RUNNING -> "正在解密..."
            STATUS_DONE -> "成功"
            else -> "失败"
        }
        val statusColor = when (rec.status) {
            STATUS_DONE -> Color(0xFF43A047)
            STATUS_FAILED -> Color(0xFFE53935)
            else -> Color(0xFF888888)
        }
        Text(
            "$statusText · 大小: ${rec.fileSize} · 时长: ${rec.musicLength} · 耗时: ${rec.durationText}",
            fontSize = 12.sp,
            color = statusColor,
        )
        if (rec.decryptDate.isNotBlank()) {
            Text("日期: ${rec.decryptDate}", fontSize = 12.sp, color = Color(0xFF888888))
        }
        if (rec.errorMessage.isNotBlank()) {
            Text("错误原因: ${rec.errorMessage}", fontSize = 12.sp, color = Color(0xFFE53935))
        }
        if (rec.status == STATUS_FAILED) {
            Spacer(Modifier.height(6.dp))
            Button(onClick = onRetry) { Text("重试") }
        }
    }
}

/** SAF tree URI 映射为真实路径：primary:Music/Foo -> /storage/emulated/0/Music/Foo */
private fun treeUriToPath(uri: Uri): String? {
    val docId = try {
        DocumentsContract.getTreeDocumentId(uri)
    } catch (_: Exception) {
        return null
    }
    val split = docId.split(":", limit = 2)
    if (split.size == 2 && split[0].equals("primary", ignoreCase = true)) {
        return Environment.getExternalStorageDirectory().absolutePath + "/" + split[1]
    }
    return null
}
