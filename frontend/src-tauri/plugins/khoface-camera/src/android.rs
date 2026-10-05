//! Kotlin `CameraPlugin.nativeOnFrame` uchun JNI qabul qiluvchi.

use jni::objects::{JByteBuffer, JObject};
use jni::sys::{jboolean, jint};
use jni::JNIEnv;

use super::{deliver, Facing, YuvFrame};

fn buffer<'a>(env: &JNIEnv, b: &JByteBuffer) -> Option<&'a [u8]> {
    let ptr = env.get_direct_buffer_address(b).ok()?;
    let len = env.get_direct_buffer_capacity(b).ok()?;
    // Bufer `ImageProxy.close()` gacha yaroqli; Kotlin uni JNI chaqiruvidan keyin yopadi.
    Some(unsafe { std::slice::from_raw_parts(ptr, len) })
}

#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_kho_face_camera_CameraPlugin_nativeOnFrame(
    env: JNIEnv,
    _this: JObject,
    y: JByteBuffer,
    u: JByteBuffer,
    v: JByteBuffer,
    width: jint,
    height: jint,
    y_stride: jint,
    uv_stride: jint,
    uv_pixel_stride: jint,
    rotation: jint,
    front: jboolean,
) {
    let (Some(y), Some(u), Some(v)) = (buffer(&env, &y), buffer(&env, &u), buffer(&env, &v)) else {
        return;
    };
    // Rust panikasi JNI chegarasidan o'tsa, ilova yopiladi — kadrni tashlab yuborish yaxshiroq.
    let _ = std::panic::catch_unwind(|| {
        deliver(YuvFrame {
            width: width as u32,
            height: height as u32,
            rotation: rotation.rem_euclid(360) as u32,
            facing: if front != 0 { Facing::Front } else { Facing::Back },
            y,
            u,
            v,
            y_stride: y_stride as usize,
            uv_stride: uv_stride as usize,
            uv_pixel_stride: uv_pixel_stride.max(1) as usize,
        })
    });
}
