//! Android JNI 门面（规格第七章 UI Independent 的 Android 侧落地）。
//!
//! Kotlin 侧无法直接绑定 C ABI 的 `char**` 出参；本 crate 用 Rust 包一层，
//! 把每个入口变成「吃字节数组与字符串、吐一个 JSON 信封字符串」的 JNI 方法：
//!
//! ```json
//! {"status":0, "payload": { …mdfile.h 定义的原样 JSON… }}
//! {"status":4, "error":"未知预设…"}
//! ```
//!
//! 铁律不变：排版判定一行不在这里——所有语义决策仍来自 `mdfile.h` 的入口，
//! 本层只做参数搬运与信封包装（状态码表见 mdfile.h）。

use jni::objects::{JClass, JString};
use jni::sys::{jbyteArray, jstring};
use jni::JNIEnv;

/// 调 C ABI 入口并把 (status, json) 折成信封。status==0 时 payload 为原样 JSON。
unsafe fn envelope<F>(env: &mut JNIEnv, f: F) -> jstring
where
    F: FnOnce(&mut JNIEnv) -> Result<String, i32>,
{
    let body = match f(env) {
        Ok(json) => format!("{{\"status\":0,\"payload\":{json}}}"),
        Err(code) => {
            let msg = mdcore_ffi::mdfile_last_error();
            let text = if msg.is_null() {
                String::new()
            } else {
                let s = std::ffi::CStr::from_ptr(msg as *const _).to_string_lossy().into_owned();
                unsafe { mdcore_ffi::mdfile_free_string(msg) };
                s
            };
            let esc: String = text
                .chars()
                .map(|c| match c {
                    '"' => "\\\"".into(),
                    '\\' => "\\\\".into(),
                    '\n' => "\\n".into(),
                    c => c.to_string(),
                })
                .collect();
            format!("{{\"status\":{code},\"error\":\"{esc}\"}}")
        }
    };
    to_jstring(env, &body)
}

fn to_jstring(env: &mut JNIEnv, s: &str) -> jstring {
    let obj: JString = env.new_string(s).expect("JVM 字符串创建失败（OOM 级别，无法恢复）");
    obj.into_raw()
}

fn take_bytes(env: &mut JNIEnv, arr: jbyteArray) -> Vec<u8> {
    if arr.is_null() {
        return Vec::new();
    }
    let arr = unsafe { jni::objects::JByteArray::from_raw(arr) };
    let len = env.get_array_length(&arr).expect("非法 jbyteArray") as usize;
    let mut buf = vec![0u8; len];
    if len > 0 {
        let slice = unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut i8, len) };
        env.get_byte_array_region(&arr, 0, slice).expect("读取 jbyteArray 失败");
    }
    buf
}

fn opt_cstr(env: &mut JNIEnv, s: JString) -> Option<Vec<u8>> {
    if s.is_null() {
        return None;
    }
    let java: String = env.get_string(&s).expect("非法 Java String").into();
    let mut bytes = java.into_bytes();
    bytes.push(0);
    Some(bytes)
}

fn with_c<T>(buf: Option<Vec<u8>>, f: impl FnOnce(*const std::os::raw::c_char) -> T) -> T {
    match buf {
        Some(b) => f(b.as_ptr() as *const _),
        None => f(std::ptr::null()),
    }
}

// ── optimize ──────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn Java_com_chentaoxing_mdreader_core_MdCore_optimize(
    mut env: JNIEnv,
    _class: JClass,
    input: jbyteArray,
    preset: JString,
    newline: JString,
) -> jstring {
    envelope(&mut env, |env| unsafe {
        let bytes = take_bytes(env, input);
        let p = opt_cstr(env, preset);
        let n = opt_cstr(env, newline);
        let mut out: *mut std::os::raw::c_char = std::ptr::null_mut();
        let mut out_len: usize = 0;
        let code = mdcore_ffi::mdfile_optimize(
            bytes.as_ptr(),
            bytes.len(),
            with_c(p, |p| p),
            with_c(n, |n| n),
            &mut out,
            &mut out_len,
        );
        cabi_json(env, code, out, out_len)
    })
}

// ── analyze ───────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn Java_com_chentaoxing_mdreader_core_MdCore_analyze(
    mut env: JNIEnv,
    _class: JClass,
    input: jbyteArray,
    preset: JString,
) -> jstring {
    envelope(&mut env, |env| unsafe {
        let bytes = take_bytes(env, input);
        let p = opt_cstr(env, preset);
        let mut out: *mut std::os::raw::c_char = std::ptr::null_mut();
        let mut out_len: usize = 0;
        let code =
            mdcore_ffi::mdfile_analyze(bytes.as_ptr(), bytes.len(), with_c(p, |p| p), &mut out, &mut out_len);
        cabi_json(env, code, out, out_len)
    })
}

// ── preview ───────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn Java_com_chentaoxing_mdreader_core_MdCore_preview(
    mut env: JNIEnv,
    _class: JClass,
    input: jbyteArray,
    preset: JString,
    rule_id: JString,
) -> jstring {
    envelope(&mut env, |env| unsafe {
        let bytes = take_bytes(env, input);
        let p = opt_cstr(env, preset);
        let r = opt_cstr(env, rule_id);
        let mut out: *mut std::os::raw::c_char = std::ptr::null_mut();
        let mut out_len: usize = 0;
        let code = mdcore_ffi::mdfile_preview(
            bytes.as_ptr(),
            bytes.len(),
            with_c(p, |p| p),
            with_c(r, |r| r),
            &mut out,
            &mut out_len,
        );
        cabi_json(env, code, out, out_len)
    })
}

// ── rebuild ───────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn Java_com_chentaoxing_mdreader_core_MdCore_rebuild(
    mut env: JNIEnv,
    _class: JClass,
    original: jbyteArray,
    preset: JString,
    decisions_json: JString,
) -> jstring {
    envelope(&mut env, |env| unsafe {
        let bytes = take_bytes(env, original);
        let p = opt_cstr(env, preset);
        let d = opt_cstr(env, decisions_json);
        let mut out: *mut std::os::raw::c_char = std::ptr::null_mut();
        let mut out_len: usize = 0;
        let code = mdcore_ffi::mdfile_rebuild(
            bytes.as_ptr(),
            bytes.len(),
            with_c(p, |p| p),
            with_c(d, |d| d),
            &mut out,
            &mut out_len,
        );
        cabi_json(env, code, out, out_len)
    })
}

// ── rules / version ───────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn Java_com_chentaoxing_mdreader_core_MdCore_rulesJson(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    envelope(&mut env, |env| unsafe {
        let mut out: *mut std::os::raw::c_char = std::ptr::null_mut();
        let mut out_len: usize = 0;
        let code = mdcore_ffi::mdfile_rules_json(&mut out, &mut out_len);
        cabi_json(env, code, out, out_len)
    })
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_chentaoxing_mdreader_core_MdCore_engineVersionNative(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let v = mdcore_ffi::mdfile_engine_version();
    let s = if v.is_null() {
        String::new()
    } else {
        std::ffi::CStr::from_ptr(v).to_string_lossy().into_owned()
    };
    to_jstring(&mut env, &s)
}

// ── 公共：状态码 → 信封 ───────────────────────────────────────────────

/// 成功时取出 JSON 字符串并释放；失败时返回 Err(状态码)（错误消息由 envelope 补）。
unsafe fn cabi_json(_env: &mut JNIEnv, code: i32, out: *mut std::os::raw::c_char, len: usize) -> Result<String, i32> {
    if code != 0 {
        if !out.is_null() {
            // 防御：按契约失败时 out 为 NULL；若未来变化也不泄漏
            unsafe { mdcore_ffi::mdfile_free_string(out) };
        }
        return Err(code);
    }
    if out.is_null() {
        return Ok("null".to_string());
    }
    let bytes = unsafe { std::slice::from_raw_parts(out as *const u8, len) };
    let json = String::from_utf8_lossy(bytes).into_owned();
    unsafe { mdcore_ffi::mdfile_free_string(out) };
    Ok(json)
}
