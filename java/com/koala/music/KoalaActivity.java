package com.koala.music;

import android.app.NativeActivity;
import android.content.Intent;
import android.net.Uri;
import android.os.Build;
import android.os.Environment;
import android.provider.DocumentsContract;
import android.provider.Settings;

/**
 * NativeActivity 子类：
 * 1. 通过 SAF (ACTION_OPEN_DOCUMENT_TREE) 调起系统目录选择器；
 * 2. 在 onActivityResult 中把 tree URI 映射回真实文件系统路径并暂存；
 * 3. 提供"所有文件访问"(MANAGE_EXTERNAL_STORAGE) 的检测与引导授权；
 * Rust 侧通过 JNI 调用这些静态方法（见 src/android_bridge.rs）。
 */
public class KoalaActivity extends NativeActivity {
    private static final int REQUEST_PICK_FOLDER = 1001;

    private static KoalaActivity instance;
    private static volatile String pickedPath = null;

    @Override
    protected void onCreate(android.os.Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        instance = this;
    }

    @Override
    protected void onDestroy() {
        instance = null;
        super.onDestroy();
    }

    /** 调起系统目录选择器（UI 线程执行）。 */
    public static void pickFolder() {
        final KoalaActivity activity = instance;
        if (activity == null) {
            return;
        }
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
                try {
                    activity.startActivityForResult(intent, REQUEST_PICK_FOLDER);
                } catch (Exception ignored) {
                }
            }
        });
    }

    /** 取走并清空最近一次选择的目录路径（未选择时返回 null）。 */
    public static String takePickedPath() {
        String path = pickedPath;
        pickedPath = null;
        return path;
    }

    /** Android 11+ 检查"所有文件访问"权限；低版本返回 true（WRITE_EXTERNAL_STORAGE 已声明）。 */
    public static boolean hasStoragePermission() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            return Environment.isExternalStorageManager();
        }
        return true;
    }

    /** 跳转系统设置引导用户授予"所有文件访问"权限。 */
    public static void requestStoragePermission() {
        final KoalaActivity activity = instance;
        if (activity == null) {
            return;
        }
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                try {
                    Intent intent = new Intent(
                            Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                            Uri.parse("package:" + activity.getPackageName()));
                    activity.startActivity(intent);
                } catch (Exception e) {
                    try {
                        activity.startActivity(
                                new Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION));
                    } catch (Exception ignored) {
                    }
                }
            }
        });
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != REQUEST_PICK_FOLDER
                || resultCode != RESULT_OK
                || data == null
                || data.getData() == null) {
            return;
        }
        pickedPath = treeUriToPath(data.getData());
    }

    /**
     * 把 SAF tree URI 映射回真实路径。
     * 例：content://...tree/primary%3AMusic%2FFoo/document/...
     * documentId = "primary:Music/Foo" -> "/storage/emulated/0/Music/Foo"
     */
    private static String treeUriToPath(Uri treeUri) {
        try {
            String docId = DocumentsContract.getTreeDocumentId(treeUri);
            if (docId == null) {
                return null;
            }
            String[] split = docId.split(":", 2);
            if (split.length == 2 && "primary".equalsIgnoreCase(split[0])) {
                return Environment.getExternalStorageDirectory().getAbsolutePath()
                        + "/" + split[1];
            }
        } catch (Exception ignored) {
        }
        return null;
    }
}
