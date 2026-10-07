//! three.js-compatible transform math in f64.
//!
//! JS numbers are f64 and three.js keeps positions/rotations/matrices in f64,
//! so the simulation does too (world z reaches ~1e4, where f32 would already
//! lose 1e-3). Euler order is three's default `XYZ`.

use bevy::math::{DMat3, DMat4, DQuat, DVec3};

/// `Quaternion.setFromEuler(e)` for order XYZ.
pub fn quat_from_euler(e: DVec3) -> DQuat {
    let (c1, c2, c3) = ((e.x / 2.0).cos(), (e.y / 2.0).cos(), (e.z / 2.0).cos());
    let (s1, s2, s3) = ((e.x / 2.0).sin(), (e.y / 2.0).sin(), (e.z / 2.0).sin());
    DQuat::from_xyzw(
        s1 * c2 * c3 + c1 * s2 * s3,
        c1 * s2 * c3 - s1 * c2 * s3,
        c1 * c2 * s3 + s1 * s2 * c3,
        c1 * c2 * c3 - s1 * s2 * s3,
    )
}

/// `Euler.setFromRotationMatrix(m, "XYZ")` on a pure rotation matrix.
pub fn euler_from_rotation(m: &DMat3) -> DVec3 {
    // three's m11.. are row/column: m11=c0.x m12=c1.x m13=c2.x, m22=c1.y m23=c2.y, m32=c1.z m33=c2.z
    let (m11, m12, m13) = (m.x_axis.x, m.y_axis.x, m.z_axis.x);
    let (m22, m23) = (m.y_axis.y, m.z_axis.y);
    let (m32, m33) = (m.y_axis.z, m.z_axis.z);
    let y = m13.clamp(-1.0, 1.0).asin();
    if m13.abs() < 0.999_999_9 {
        DVec3::new((-m23).atan2(m33), y, (-m12).atan2(m11))
    } else {
        DVec3::new(m32.atan2(m22), y, 0.0)
    }
}

/// `Euler.setFromQuaternion(q, "XYZ")`.
pub fn euler_from_quat(q: DQuat) -> DVec3 {
    euler_from_rotation(&DMat3::from_quat(q))
}

/// `Matrix4.compose(position, quaternion(euler), scale)`.
pub fn compose(pos: DVec3, euler: DVec3, scale: DVec3) -> DMat4 {
    DMat4::from_scale_rotation_translation(scale, quat_from_euler(euler), pos)
}

/// `Matrix4.decompose()` (three's version: sign of det goes to scale.x).
pub fn decompose(m: &DMat4) -> (DVec3, DQuat, DVec3) {
    let mut sx = m.x_axis.truncate().length();
    let sy = m.y_axis.truncate().length();
    let sz = m.z_axis.truncate().length();
    if m.determinant() < 0.0 {
        sx = -sx;
    }
    let pos = m.w_axis.truncate();
    let rot = DMat3::from_cols(m.x_axis.truncate() / sx, m.y_axis.truncate() / sy, m.z_axis.truncate() / sz);
    (pos, quat_from_rotation(&rot), DVec3::new(sx, sy, sz))
}

/// `Quaternion.setFromRotationMatrix` (three's branch structure).
pub fn quat_from_rotation(m: &DMat3) -> DQuat {
    let (m11, m12, m13) = (m.x_axis.x, m.y_axis.x, m.z_axis.x);
    let (m21, m22, m23) = (m.x_axis.y, m.y_axis.y, m.z_axis.y);
    let (m31, m32, m33) = (m.x_axis.z, m.y_axis.z, m.z_axis.z);
    let trace = m11 + m22 + m33;
    if trace > 0.0 {
        let s = 0.5 / (trace + 1.0).sqrt();
        DQuat::from_xyzw((m32 - m23) * s, (m13 - m31) * s, (m21 - m12) * s, 0.25 / s)
    } else if m11 > m22 && m11 > m33 {
        let s = 2.0 * (1.0 + m11 - m22 - m33).sqrt();
        DQuat::from_xyzw(0.25 * s, (m12 + m21) / s, (m13 + m31) / s, (m32 - m23) / s)
    } else if m22 > m33 {
        let s = 2.0 * (1.0 + m22 - m11 - m33).sqrt();
        DQuat::from_xyzw((m12 + m21) / s, 0.25 * s, (m23 + m32) / s, (m13 - m31) / s)
    } else {
        let s = 2.0 * (1.0 + m33 - m11 - m22).sqrt();
        DQuat::from_xyzw((m13 + m31) / s, (m23 + m32) / s, 0.25 * s, (m21 - m12) / s)
    }
}
