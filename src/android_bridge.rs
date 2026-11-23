
//! Android JNI 桥接：调起系统目录选择器、取回选择结果、管理"所有文件访问"权限。
//! Java 侧实现见 java/com/koala/music/KoalaActivity.java。

use jni::objects::{JObject, JString};
use jni::JavaVM;

/// 拿到当前线程的 JNIEnv、NativeActivity 实例和其运行时 Class，
/// 然后执行 `f`。任何一步失败都返回 None（不 panic，不影响 UI）。
fn with_activity<T>(
    f: impl FnOnce(&mut jni::JNIEnv, &JObject, jni::objects::JClass) -> Option<T>,
) -> Option<T> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm() as *mut _) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    // ndk_context 里的 context 就是我们的 KoalaActivity 实例（NativeActivity），
    // 直接取它的运行时类调用静态方法，绕开原生线程的 ClassLoader 找不到应用类的问题。
    let activity = unsafe { JObject::from_raw(ctx.context() as *mut _) };
    let class = env.get_object_class(&activity).ok()?;
    f(&mut env, &activity, class)
}

/// 调起系统目录选择器（ACTION_OPEN_DOCUMENT_TREE）。
pub fn pick_folder() {
    with_activity(|env, _activity, class| {
        env.call_static_method(class, "pickFolder", "()V", &[]).ok().map(|_| ())
    });
}

/// 取走最近一次选择的目录路径；用户取消或尚未返回时为 None。
pub fn take_picked_path() -> Option<String> {
    with_activity(|env, _activity, class| {
        let value = env
            .call_static_method(class, "takePickedPath", "()Ljava/lang/String;", &[])
            .ok()?;
        let obj = value.l().ok()?;
        if obj.is_null() {
            return None;
        }
        let s: JString = obj.into();
        let path = env.get_string(&s).ok()?.to_string_lossy().into_owned();
        Some(path)
    })
    .flatten()
}

/// Android 11+ 是否已授予"所有文件访问"；低版本恒为 true。
pub fn has_storage_permission() -> bool {
    with_activity(|env, _activity, class| {
        env.call_static_method(class, "hasStoragePermission", "()Z", &[])
            .ok()?
            .z()
            .ok()
    })
    .unwrap_or(false)
}

/// 跳转系统设置引导用户授予"所有文件访问"权限。
pub fn request_storage_permission() {
    with_activity(|env, _activity, class| {
        env.call_static_method(class, "requestStoragePermission", "()V", &[])
            .ok()
            .map(|_| ())
    });
}
