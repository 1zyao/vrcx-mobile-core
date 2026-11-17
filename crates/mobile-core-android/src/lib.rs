//! Android JNI entry point for the phone core.
//!
//! The core itself (`vrcx-0-mobile-core`) is platform-neutral: it can produce a static library, which is
//! what iOS links. This crate exists only because Android needs a `cdylib` named `vrcx_0_mobile` for
//! `System.loadLibrary`, and because building a `cdylib` for an Apple target tries to produce a dynamic
//! library iOS cannot load anyway - which is what broke the iOS CI with a missing
//! `___chkstk_darwin` builtin. One crate per artefact keeps both hosts honest.

#[cfg(target_os = "android")]
mod android {
    use std::path::Path;

    /// JNI entry point for the Android app.
    ///
    /// Provisional symbol: it matches a `NativeCore` object in the
    /// `io.github.vrcmteam.vrcm.core` package on the Kotlin side. Rename the class and this must be
    /// renamed with it.
    #[no_mangle]
    pub extern "system" fn Java_io_github_vrcmteam_vrcm_core_NativeCore_start(
        mut env: jni::JNIEnv,
        _class: jni::objects::JClass,
        files_dir: jni::objects::JString,
        device_name: jni::objects::JString,
    ) -> jni::sys::jstring {
        let result = (|| -> Result<String, String> {
            let files_dir: String = env
                .get_string(&files_dir)
                .map_err(|error| format!("read filesDir: {error}"))?
                .into();
            let device_name: String = env
                .get_string(&device_name)
                .map_err(|error| format!("read deviceName: {error}"))?
                .into();
            let endpoint = vrcx_0_mobile_core::start_core(Path::new(&files_dir), &device_name)?;
            serde_json::to_string(&endpoint).map_err(|error| error.to_string())
        })();

        let reply = match result {
            Ok(json) => json,
            Err(message) => serde_json::json!({ "error": message }).to_string(),
        };
        env.new_string(reply)
            .map(|value| value.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }
}
